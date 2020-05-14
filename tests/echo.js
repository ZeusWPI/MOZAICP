const WebSocket = require('ws');

const ws = new WebSocket('ws://localhost:6668');

const duplex = WebSocket.createWebSocketStream(ws, { encoding: 'utf8' });

duplex.pipe(process.stdout);
process.stdin.pipe(duplex);
