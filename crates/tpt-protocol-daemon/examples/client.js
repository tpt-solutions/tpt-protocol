// Minimal browser/Node.js consumer for the TPT Protocol daemon's TUP WebSocket
// stream. No Rust required.
//
// Browser: serve this from any static host and open the console.
// Node.js:  npm install ws   then   node client.js

// --- Browser ---------------------------------------------------------------
// const ws = new WebSocket("ws://127.0.0.1:9080");
// ws.onmessage = (event) => {
//   const payload = JSON.parse(event.data);
//   if (!Array.isArray(payload)) {
//     // {"type":"lagged","dropped":n}: this client fell behind the stream.
//     console.warn("daemon dropped", payload.dropped, "messages");
//     return;
//   }
//   for (const dp of payload) {
//     console.log(dp.source_protocol, dp.device_id, dp.point_id, dp.value, dp.quality);
//   }
// };

// --- Node.js (uncomment the block below and run with node) ------------------
// const WebSocket = require("ws");
// const ws = new WebSocket("ws://127.0.0.1:9080");
// ws.on("message", (data) => {
//   const payload = JSON.parse(data);
//   if (!Array.isArray(payload)) {
//     console.warn("daemon dropped", payload.dropped, "messages");
//     return;
//   }
//   for (const dp of payload) {
//     console.log(dp.source_protocol, dp.device_id, dp.point_id, dp.value, dp.quality);
//   }
// });

console.log("See commented examples above for browser and Node.js usage.");
