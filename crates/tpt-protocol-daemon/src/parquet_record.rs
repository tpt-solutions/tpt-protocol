//! Parquet recording (`--record-parquet FILE`, feature `parquet-record`).
//!
//! One row per point, in a single table with these columns:
//!
//! | column        | type            | notes                                  |
//! |---------------|-----------------|----------------------------------------|
//! | `timestamp_ms`| int64, required | point time, milliseconds (0 if unset)  |
//! | `protocol`    | string          | source protocol                        |
//! | `device`      | string          | device id                              |
//! | `point`       | string          | point id                               |
//! | `quality`     | string          | good, bad, stale or uncertain          |
//! | `unit`        | string, nullable|                                        |
//! | `int_value`   | int64, nullable | set for integer values                 |
//! | `float_value` | double, nullable| set for floats (finite only)           |
//! | `bool_value`  | boolean, nullable| set for booleans                      |
//! | `text_value`  | string, nullable| set for text                           |
//! | `bytes_value` | binary, nullable| set for raw bytes                      |
//!
//! Each point sets exactly one of the four value columns. Rows are written in row
//! groups of `ROW_GROUP_ROWS`. Parquet keeps its table of contents (the footer)
//! at the end of the file, so the file can be read only after the source closes.
//! A daemon stopped with Ctrl-C leaves a file with no footer, which readers reject.
//! The MCAP recorder does not have this problem; use it for long captures.
//!
//! Without the `parquet-record` feature, `--record-parquet` is refused with a clear error.

use std::path::Path;

use anyhow::Result;
use tokio::sync::broadcast;
use tpt_protocol_tup::DataPoint;

/// Rows per row group. A row group is written, and its memory freed, once this many
/// points have arrived.
#[cfg(feature = "parquet-record")]
pub const ROW_GROUP_ROWS: usize = 4096;

/// Opens `path` for writing and records every point from `rx` until it closes.
/// The file is created here, so a bad path is a startup error.
#[cfg(feature = "parquet-record")]
pub fn spawn_parquet(path: &Path, rx: broadcast::Receiver<DataPoint>) -> Result<()> {
    use anyhow::anyhow;
    let file = std::fs::File::create(path)
        .map_err(|e| anyhow!("cannot create {}: {e}", path.display()))?;
    let writer = imp::new_writer(file)?;
    tracing::info!("recording Parquet to {}", path.display());
    tokio::task::spawn_blocking(move || {
        if let Err(e) = imp::write_points(writer, rx) {
            tracing::error!("Parquet recording stopped: {e:#}");
        }
    });
    Ok(())
}

/// Without the `parquet-record` feature there is no writer to start.
#[cfg(not(feature = "parquet-record"))]
pub fn spawn_parquet(_path: &Path, _rx: broadcast::Receiver<DataPoint>) -> Result<()> {
    anyhow::bail!("--record-parquet needs a daemon built with the `parquet-record` feature")
}

/// The flat row written for one point. Exactly one of the four value fields is set.
#[cfg(feature = "parquet-record")]
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub timestamp_ms: i64,
    pub protocol: String,
    pub device: String,
    pub point: String,
    pub quality: String,
    pub unit: Option<String>,
    pub int_value: Option<i64>,
    pub float_value: Option<f64>,
    pub bool_value: Option<bool>,
    pub text_value: Option<String>,
    pub bytes_value: Option<Vec<u8>>,
}

#[cfg(feature = "parquet-record")]
impl Row {
    /// The row for a point. Non-finite floats are kept out of `float_value`; such a
    /// point has no value column set, so readers see it as null.
    pub fn from_point(dp: &DataPoint) -> Self {
        use tpt_protocol_tup::TupValue;
        let mut row = Row {
            timestamp_ms: i64::try_from(dp.timestamp).unwrap_or(i64::MAX),
            protocol: dp.source_protocol.as_str().to_string(),
            device: dp.device_id.clone(),
            point: dp.point_id.clone(),
            quality: dp.quality.as_str().to_string(),
            unit: dp.unit.clone(),
            int_value: None,
            float_value: None,
            bool_value: None,
            text_value: None,
            bytes_value: None,
        };
        match &dp.value {
            TupValue::Int(v) => row.int_value = Some(*v),
            TupValue::Float(v) if v.is_finite() => row.float_value = Some(*v),
            TupValue::Float(_) => {}
            TupValue::Bool(v) => row.bool_value = Some(*v),
            TupValue::Text(v) => row.text_value = Some(v.clone()),
            TupValue::Bytes(v) => row.bytes_value = Some(v.clone()),
        }
        row
    }
}

#[cfg(feature = "parquet-record")]
mod imp {
    use std::fs::File;
    use std::io::{Seek, Write};
    use std::sync::Arc;

    use anyhow::{anyhow, Result};
    #[cfg(test)]
    use parquet::basic::{Repetition, Type as PhysicalType};
    use parquet::data_type::{BoolType, ByteArray, ByteArrayType, DataType, DoubleType, Int64Type};
    use parquet::file::properties::WriterProperties;
    use parquet::file::writer::{SerializedFileWriter, SerializedRowGroupWriter};
    use parquet::schema::parser::parse_message_type;
    use tokio::sync::broadcast;
    use tokio::sync::broadcast::error::RecvError;
    use tpt_protocol_tup::DataPoint;

    use super::{Row, ROW_GROUP_ROWS};

    const SCHEMA: &str = "
        message tpt_point {
            required int64 timestamp_ms;
            required binary protocol (UTF8);
            required binary device (UTF8);
            required binary point (UTF8);
            required binary quality (UTF8);
            optional binary unit (UTF8);
            optional int64 int_value;
            optional double float_value;
            optional boolean bool_value;
            optional binary text_value (UTF8);
            optional binary bytes_value;
        }";

    pub fn new_writer(file: File) -> Result<SerializedFileWriter<File>> {
        let schema =
            Arc::new(parse_message_type(SCHEMA).map_err(|e| anyhow!("bad Parquet schema: {e}"))?);
        let props = Arc::new(WriterProperties::builder().build());
        SerializedFileWriter::new(file, schema, props)
            .map_err(|e| anyhow!("cannot start a Parquet file: {e}"))
    }

    /// Writes one required column. `values` has one entry per row.
    fn required<W, T>(rg: &mut SerializedRowGroupWriter<'_, W>, values: &[T::T]) -> Result<()>
    where
        W: Write + Send,
        T: DataType,
    {
        let mut column = rg
            .next_column()?
            .ok_or_else(|| anyhow!("schema has fewer columns than the writer expects"))?;
        column.typed::<T>().write_batch(values, None, None)?;
        column.close()?;
        Ok(())
    }

    /// Writes one optional column. `present` has one entry per row; `values` holds
    /// only the present ones, in order.
    fn optional<W, T>(
        rg: &mut SerializedRowGroupWriter<'_, W>,
        present: &[bool],
        values: &[T::T],
    ) -> Result<()>
    where
        W: Write + Send,
        T: DataType,
    {
        let mut column = rg
            .next_column()?
            .ok_or_else(|| anyhow!("schema has fewer columns than the writer expects"))?;
        let def_levels: Vec<i16> = present.iter().map(|&p| i16::from(p)).collect();
        column
            .typed::<T>()
            .write_batch(values, Some(&def_levels), None)?;
        column.close()?;
        Ok(())
    }

    fn text(value: &str) -> ByteArray {
        ByteArray::from(value.as_bytes().to_vec())
    }

    /// Writes one row group holding `rows`, in schema order.
    fn write_group<W: Write + Seek + Send>(
        writer: &mut SerializedFileWriter<W>,
        rows: &[Row],
    ) -> Result<()> {
        let mut rg = writer.next_row_group()?;

        let timestamps: Vec<i64> = rows.iter().map(|r| r.timestamp_ms).collect();
        required::<W, Int64Type>(&mut rg, &timestamps)?;
        let protocols: Vec<ByteArray> = rows.iter().map(|r| text(&r.protocol)).collect();
        required::<W, ByteArrayType>(&mut rg, &protocols)?;
        let devices: Vec<ByteArray> = rows.iter().map(|r| text(&r.device)).collect();
        required::<W, ByteArrayType>(&mut rg, &devices)?;
        let points: Vec<ByteArray> = rows.iter().map(|r| text(&r.point)).collect();
        required::<W, ByteArrayType>(&mut rg, &points)?;
        let qualities: Vec<ByteArray> = rows.iter().map(|r| text(&r.quality)).collect();
        required::<W, ByteArrayType>(&mut rg, &qualities)?;

        let units: Vec<bool> = rows.iter().map(|r| r.unit.is_some()).collect();
        let unit_values: Vec<ByteArray> = rows
            .iter()
            .filter_map(|r| r.unit.as_deref().map(text))
            .collect();
        optional::<W, ByteArrayType>(&mut rg, &units, &unit_values)?;

        let ints: Vec<bool> = rows.iter().map(|r| r.int_value.is_some()).collect();
        let int_values: Vec<i64> = rows.iter().filter_map(|r| r.int_value).collect();
        optional::<W, Int64Type>(&mut rg, &ints, &int_values)?;

        let floats: Vec<bool> = rows.iter().map(|r| r.float_value.is_some()).collect();
        let float_values: Vec<f64> = rows.iter().filter_map(|r| r.float_value).collect();
        optional::<W, DoubleType>(&mut rg, &floats, &float_values)?;

        let bools: Vec<bool> = rows.iter().map(|r| r.bool_value.is_some()).collect();
        let bool_values: Vec<bool> = rows.iter().filter_map(|r| r.bool_value).collect();
        optional::<W, BoolType>(&mut rg, &bools, &bool_values)?;

        let texts: Vec<bool> = rows.iter().map(|r| r.text_value.is_some()).collect();
        let text_values: Vec<ByteArray> = rows
            .iter()
            .filter_map(|r| r.text_value.as_deref().map(text))
            .collect();
        optional::<W, ByteArrayType>(&mut rg, &texts, &text_values)?;

        let bytes: Vec<bool> = rows.iter().map(|r| r.bytes_value.is_some()).collect();
        let bytes_values: Vec<ByteArray> = rows
            .iter()
            .filter_map(|r| r.bytes_value.clone().map(ByteArray::from))
            .collect();
        optional::<W, ByteArrayType>(&mut rg, &bytes, &bytes_values)?;

        rg.close()?;
        Ok(())
    }

    /// Writes points until the channel closes, then the last row group and the footer.
    pub fn write_points<W: Write + Seek + Send>(
        mut writer: SerializedFileWriter<W>,
        mut rx: broadcast::Receiver<DataPoint>,
    ) -> Result<()> {
        let mut rows: Vec<Row> = Vec::with_capacity(ROW_GROUP_ROWS);
        loop {
            match rx.blocking_recv() {
                Ok(dp) => {
                    rows.push(Row::from_point(&dp));
                    if rows.len() >= ROW_GROUP_ROWS {
                        write_group(&mut writer, &rows)?;
                        rows.clear();
                    }
                }
                Err(RecvError::Lagged(n)) => {
                    tracing::warn!("parquet: fell behind and skipped {n} points");
                }
                Err(RecvError::Closed) => break,
            }
        }
        if !rows.is_empty() {
            write_group(&mut writer, &rows)?;
        }
        writer.close()?;
        Ok(())
    }

    /// The physical type of each schema column, in order, for tests.
    #[cfg(test)]
    pub fn column_types() -> Vec<PhysicalType> {
        let schema = parse_message_type(SCHEMA).expect("schema parses");
        schema
            .get_fields()
            .iter()
            .map(|f| f.get_physical_type())
            .collect()
    }

    /// Repetition of each schema column, in order, for tests.
    #[cfg(test)]
    pub fn column_repetitions() -> Vec<Repetition> {
        let schema = parse_message_type(SCHEMA).expect("schema parses");
        schema
            .get_fields()
            .iter()
            .map(|f| f.get_basic_info().repetition())
            .collect()
    }
}

#[cfg(all(test, feature = "parquet-record"))]
mod tests {
    use super::*;
    use parquet::basic::Repetition;
    use parquet::file::reader::{FileReader, SerializedFileReader};
    use tpt_protocol_tup::{Quality, SourceProtocol, TupValue};

    fn point(i: i64, value: TupValue) -> DataPoint {
        DataPoint::new(
            SourceProtocol::Modbus,
            "plc-1",
            format!("holding_register[{i}]"),
            1_753_900_000_000 + i as u64,
            value,
            Quality::Good,
        )
    }

    /// Records `points` through the real writer thread and returns the file bytes.
    fn record(path: &std::path::Path, points: Vec<DataPoint>) {
        let file = std::fs::File::create(path).unwrap();
        let writer = imp::new_writer(file).unwrap();
        let (tx, rx) = broadcast::channel::<DataPoint>(points.len() + 1);
        for p in points {
            tx.send(p).unwrap();
        }
        drop(tx);
        imp::write_points(writer, rx).unwrap();
    }

    #[test]
    fn schema_has_the_documented_columns() {
        assert_eq!(imp::column_types().len(), 11);
        assert_eq!(imp::column_repetitions()[0], Repetition::REQUIRED);
        assert_eq!(imp::column_repetitions()[5], Repetition::OPTIONAL);
    }

    #[test]
    fn a_row_takes_its_value_from_the_matching_column() {
        let row = Row::from_point(&point(1, TupValue::Int(-7)));
        assert_eq!(row.int_value, Some(-7));
        assert_eq!(row.float_value, None);
        assert_eq!(row.protocol, "modbus");

        let row = Row::from_point(&point(2, TupValue::Float(f64::NAN)));
        assert_eq!(row.float_value, None);
        assert_eq!(row.int_value, None);
    }

    #[test]
    fn recorded_file_has_one_row_per_point_across_row_groups() {
        let dir = std::env::temp_dir().join(format!("tpt-parquet-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("points.parquet");
        let count = ROW_GROUP_ROWS * 2 + 3;
        let points: Vec<DataPoint> = (0..count)
            .map(|i| point(i as i64, TupValue::Int(i as i64)))
            .collect();
        record(&path, points);

        let reader = SerializedFileReader::new(std::fs::File::open(&path).unwrap()).unwrap();
        let meta = reader.metadata();
        assert_eq!(meta.file_metadata().num_rows(), count as i64);
        assert_eq!(meta.num_row_groups(), 3);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Writes a small mixed-type file for an independent reader (pyarrow). Run with
    /// `TPT_PARQUET_SAMPLE=<path> cargo test -p tpt-protocol-daemon --features
    /// parquet-record -- --ignored parquet_sample`, then read it with `pyarrow`.
    #[test]
    #[ignore = "writes a sample file: set TPT_PARQUET_SAMPLE"]
    fn parquet_sample() {
        let Ok(path) = std::env::var("TPT_PARQUET_SAMPLE") else {
            return;
        };
        let mut unit_point = point(1, TupValue::Float(21.5));
        unit_point.unit = Some("Cel".into());
        let points = vec![
            point(0, TupValue::Int(-7)),
            unit_point,
            point(2, TupValue::Bool(true)),
            point(3, TupValue::Text("ok".into())),
            point(4, TupValue::Bytes(vec![1, 2, 3])),
            point(5, TupValue::Float(f64::NAN)),
        ];
        record(std::path::Path::new(&path), points);
    }
}
