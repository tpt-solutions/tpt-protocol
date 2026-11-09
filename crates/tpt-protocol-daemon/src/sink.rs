//! Sinks: send the TUP points somewhere other than the local WebSocket.
//!
//! The MQTT sink (feature `mqtt`) publishes each point as JSON to a broker, on the
//! topic `<prefix>/<protocol>/<device>/<point>`. It uses QoS 0 and does not retain.
//! Publishing never waits: when the broker cannot keep up, points are dropped and
//! counted in the log, so a slow broker cannot stall the devices. The connection
//! reconnects by itself.
//!
//! Without the `mqtt` feature, `--mqtt` is refused with a clear error.

use anyhow::Result;
use tokio::sync::broadcast;
use tpt_protocol_tup::DataPoint;

/// Starts the MQTT sink for `addr` (`HOST:PORT`, plain TCP), publishing under `prefix`.
/// Returns once the sink is running; it then runs until `rx` closes.
#[cfg(feature = "mqtt")]
pub fn spawn_mqtt(addr: &str, prefix: &str, rx: broadcast::Receiver<DataPoint>) -> Result<()> {
    let (host, port) = mqtt::parse_addr(addr)?;
    let prefix = mqtt::checked_prefix(prefix)?;
    tokio::spawn(mqtt::run(host, port, prefix, rx));
    Ok(())
}

/// Without the `mqtt` feature there is no client to start.
#[cfg(not(feature = "mqtt"))]
pub fn spawn_mqtt(_addr: &str, _prefix: &str, _rx: broadcast::Receiver<DataPoint>) -> Result<()> {
    anyhow::bail!("--mqtt needs a daemon built with the `mqtt` feature")
}

#[cfg(feature = "mqtt")]
mod mqtt {
    use std::time::Duration;

    use anyhow::{anyhow, bail, Result};
    use rumqttc::{AsyncClient, Event, MqttOptions, QoS};
    use tokio::sync::broadcast;
    use tokio::sync::broadcast::error::RecvError;
    use tpt_protocol_tup::{json, DataPoint};

    /// Pending publishes the client may hold. Beyond this, `try_publish` reports full.
    const QUEUE: usize = 256;
    const RECONNECT_DELAY: Duration = Duration::from_secs(2);

    /// Splits `HOST:PORT`. The host may be a name or an IP address.
    pub fn parse_addr(addr: &str) -> Result<(String, u16)> {
        let (host, port) = addr
            .rsplit_once(':')
            .ok_or_else(|| anyhow!("--mqtt wants HOST:PORT, got '{addr}'"))?;
        let port: u16 = port
            .parse()
            .map_err(|_| anyhow!("--mqtt port '{port}' is not a number"))?;
        if host.is_empty() {
            bail!("--mqtt host is empty");
        }
        Ok((host.to_string(), port))
    }

    /// A topic prefix must not hold MQTT wildcards or be empty.
    pub fn checked_prefix(prefix: &str) -> Result<String> {
        let trimmed = prefix.trim_matches('/');
        if trimmed.is_empty() {
            bail!("--mqtt-prefix must not be empty");
        }
        if trimmed.contains(['+', '#']) {
            bail!("--mqtt-prefix must not contain the MQTT wildcards '+' or '#'");
        }
        Ok(trimmed.to_string())
    }

    /// Topic for a point. Each segment is cleaned so a device or point id cannot add
    /// levels, wildcards or control characters to the topic.
    pub fn topic(prefix: &str, dp: &DataPoint) -> String {
        let clean = |s: &str| {
            let cleaned: String = s
                .chars()
                .map(|c| {
                    if c == '/' || c == '+' || c == '#' || c.is_control() {
                        '_'
                    } else {
                        c
                    }
                })
                .collect();
            if cleaned.is_empty() {
                "_".to_string()
            } else {
                cleaned
            }
        };
        format!(
            "{prefix}/{}/{}/{}",
            clean(dp.source_protocol.as_str()),
            clean(&dp.device_id),
            clean(&dp.point_id)
        )
    }

    /// Publishes every point from `rx` until the channel closes.
    pub async fn run(
        host: String,
        port: u16,
        prefix: String,
        mut rx: broadcast::Receiver<DataPoint>,
    ) {
        let mut options = MqttOptions::new(
            format!("tpt-daemon-{}", std::process::id()),
            host.clone(),
            port,
        );
        options.set_keep_alive(Duration::from_secs(30));
        let (client, mut eventloop) = AsyncClient::new(options, QUEUE);
        tracing::info!("mqtt: publishing to {host}:{port} under '{prefix}/'");

        // rumqttc only connects and sends while its event loop is polled. Errors are
        // logged and the loop carries on; the next poll reconnects.
        tokio::spawn(async move {
            loop {
                match eventloop.poll().await {
                    Ok(Event::Incoming(_)) | Ok(Event::Outgoing(_)) => {}
                    Err(e) => {
                        tracing::warn!("mqtt: {e}; retrying");
                        tokio::time::sleep(RECONNECT_DELAY).await;
                    }
                }
            }
        });

        let mut dropped: u64 = 0;
        loop {
            match rx.recv().await {
                Ok(dp) => {
                    let payload = match json::to_json_string(&dp) {
                        Ok(text) => text,
                        Err(e) => {
                            tracing::warn!("mqtt: cannot encode a point: {e}");
                            continue;
                        }
                    };
                    if client
                        .try_publish(topic(&prefix, &dp), QoS::AtMostOnce, false, payload)
                        .is_err()
                    {
                        dropped += 1;
                        if dropped == 1 || dropped % 1000 == 0 {
                            tracing::warn!("mqtt: queue full or broker unreachable; {dropped} points dropped so far");
                        }
                    }
                }
                Err(RecvError::Lagged(n)) => {
                    tracing::warn!("mqtt: fell behind and skipped {n} points");
                }
                Err(RecvError::Closed) => return,
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use tpt_protocol_tup::{Quality, SourceProtocol, TupValue};

        #[test]
        fn addresses_are_split_and_checked() {
            assert_eq!(
                parse_addr("broker.local:1883").unwrap(),
                ("broker.local".into(), 1883)
            );
            assert_eq!(
                parse_addr("127.0.0.1:1883").unwrap(),
                ("127.0.0.1".into(), 1883)
            );
            assert!(parse_addr("no-port").is_err());
            assert!(parse_addr(":1883").is_err());
            assert!(parse_addr("host:notaport").is_err());
        }

        #[test]
        fn prefix_is_trimmed_and_wildcards_refused() {
            assert_eq!(checked_prefix("/tpt/site-1/").unwrap(), "tpt/site-1");
            assert!(checked_prefix("").is_err());
            assert!(checked_prefix("tpt/#").is_err());
            assert!(checked_prefix("tpt/+").is_err());
        }

        #[test]
        fn topic_levels_cannot_be_injected() {
            let dp = DataPoint::new(
                SourceProtocol::Modbus,
                "plc/1#",
                "tank+level",
                1,
                TupValue::Int(1),
                Quality::Good,
            );
            assert_eq!(topic("tpt", &dp), "tpt/modbus/plc_1_/tank_level");
        }
    }
}

#[cfg(all(test, feature = "mqtt"))]
mod integration {
    //! Needs a running broker. Set `TPT_MQTT_TEST_ADDR` (HOST:PORT) and run:
    //! `cargo test -p tpt-protocol-daemon --features mqtt -- --ignored mqtt`.
    use std::time::Duration;

    use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
    use tokio::sync::broadcast;
    use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

    #[tokio::test]
    #[ignore = "needs a broker: set TPT_MQTT_TEST_ADDR (HOST:PORT)"]
    async fn mqtt_publishes_points_to_a_broker() {
        let addr = std::env::var("TPT_MQTT_TEST_ADDR").expect("TPT_MQTT_TEST_ADDR");
        let (host, port) = super::mqtt::parse_addr(&addr).unwrap();

        // An independent subscriber, on its own client.
        let mut sub_options = MqttOptions::new("tpt-test-subscriber", host.clone(), port);
        sub_options.set_keep_alive(Duration::from_secs(5));
        let (sub, mut sub_loop) = AsyncClient::new(sub_options, 32);
        sub.subscribe("tpt-test/#", QoS::AtMostOnce).await.unwrap();

        let (tx, rx) = broadcast::channel::<DataPoint>(16);
        super::spawn_mqtt(&addr, "tpt-test", rx).unwrap();

        // Keep sending until the subscriber sees one; the first publish can race the
        // connection setup.
        let point = DataPoint::new(
            SourceProtocol::Modbus,
            "plc-1",
            "holding_register[1]",
            1,
            TupValue::Int(10),
            Quality::Good,
        );
        let received = tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                let _ = tx.send(point.clone());
                let poll = tokio::time::timeout(Duration::from_millis(300), sub_loop.poll()).await;
                if let Ok(Ok(Event::Incoming(Packet::Publish(p)))) = poll {
                    return p;
                }
            }
        })
        .await
        .expect("a point should arrive within 20 s");

        assert_eq!(received.topic, "tpt-test/modbus/plc-1/holding_register[1]");
        let body = String::from_utf8(received.payload.to_vec()).unwrap();
        assert!(body.contains("\"value\":10"), "{body}");
    }
}
