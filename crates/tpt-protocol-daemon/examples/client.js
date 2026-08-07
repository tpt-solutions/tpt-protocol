// Minimal browser/Node.js consumer for the TPT Protocol daemon's TUP WebSocket
// stream. No Rust required.
//
// Browser: serve this from any static host and open the console.
// Node.js:  npm install ws   then   node client.js

// --- Browser ---------------------------------------------------------------
// const ws = new WebSocket("ws://127.0.0.1:9080");
// ws.onmessage = (event) => {
//   for (const dp of JSON.parse(event.data)) {
//     console.log(dp.source_protocol, dp.device_id, dp.point_id, dp.value, dp.quality);
//   }
// };

// --- Node.js (uncomment the block below and run with node) ------------------
// const WebSocket = require("ws");
// const ws = new WebSocket("ws://127.0.0.1:9080");
// ws.on("message", (data) => {
//   for (const dp of JSON.parse(data)) {
//     console.log(dp.source_protocol, dp.device_id, dp.point_id, dp.value, dp.quality);
//   }
// });

console.log("See commented examples above for browser and Node.js usage.");
