#!/usr/bin/env node
/**
 * Jinn Project — Simbad Server External Interoperability & Network Test Harness
 * 
 * Tests the Simbad HTTP Server from an independent external client over standard TCP sockets.
 * Can be executed against:
 * 1. A live QEMU instance running Jinn Simbad (e.g. localhost:8080 forwarded to guest port 80).
 * 2. An external mock or staging server implementing the Simbad specification.
 * 
 * Verifies:
 * - RFC 7230 / RFC 9112 message parsing
 * - GET /, GET /healthz, GET /metrics endpoints
 * - Malformed request rejection (400 Bad Request)
 * - Method not allowed (405 Method Not Allowed)
 * - Request size limits (413 Payload Too Large)
 * - Concurrent connections & teardown
 */

const net = require('net');

const HOST = process.env.SIMBAD_HOST || '127.0.0.1';
const PORT = parseInt(process.env.SIMBAD_PORT || '8080', 10);
const TIMEOUT_MS = 3000;

function sendRawHttpRequest(host, port, rawPayload) {
    return new Promise((resolve, reject) => {
        const client = new net.Socket();
        let receivedData = Buffer.alloc(0);

        client.setTimeout(TIMEOUT_MS);

        client.connect(port, host, () => {
            client.write(rawPayload);
        });

        client.on('data', (chunk) => {
            receivedData = Buffer.concat([receivedData, chunk]);
        });

        client.on('end', () => {
            client.destroy();
            resolve(receivedData.toString('utf8'));
        });

        client.on('timeout', () => {
            client.destroy();
            reject(new Error(`Connection timeout after ${TIMEOUT_MS}ms`));
        });

        client.on('error', (err) => {
            client.destroy();
            reject(err);
        });
    });
}

async function runTestSuite() {
    console.log(`================================================================================`);
    console.log(`  Simbad HTTP External Interoperability Test Suite`);
    console.log(`  Target: http://${HOST}:${PORT}`);
    console.log(`================================================================================`);

    let passed = 0;
    let failed = 0;

    async function testCase(name, fn) {
        try {
            await fn();
            console.log(`  [PASS] ${name}`);
            passed++;
        } catch (err) {
            console.log(`  [FAIL] ${name} -> ${err.message}`);
            failed++;
        }
    }

    // Test 1: GET / (Root JSON info)
    await testCase('GET / (System Status JSON)', async () => {
        const req = `GET / HTTP/1.1\r\nHost: ${HOST}\r\nConnection: close\r\n\r\n`;
        const res = await sendRawHttpRequest(HOST, PORT, req);
        if (!res.includes('200 OK')) throw new Error('Expected 200 OK');
        if (!res.includes('application/json')) throw new Error('Expected application/json header');
        if (!res.includes('Simbad Server 0.2.0')) throw new Error('Missing Simbad Server 0.2.0 metadata');
        if (!res.includes('Jinn Microkernel')) throw new Error('Missing Jinn Microkernel system descriptor');
    });

    // Test 2: GET /healthz (Healthcheck plaintext)
    await testCase('GET /healthz (Healthcheck Endpoint)', async () => {
        const req = `GET /healthz HTTP/1.1\r\nHost: ${HOST}\r\nConnection: close\r\n\r\n`;
        const res = await sendRawHttpRequest(HOST, PORT, req);
        if (!res.includes('200 OK')) throw new Error('Expected 200 OK');
        if (!res.includes('text/plain')) throw new Error('Expected text/plain header');
        if (!res.includes('healthy')) throw new Error('Expected "healthy" body');
    });

    // Test 3: GET /metrics (Prometheus exposition)
    await testCase('GET /metrics (Prometheus Exposition Format)', async () => {
        const req = `GET /metrics HTTP/1.1\r\nHost: ${HOST}\r\nConnection: close\r\n\r\n`;
        const res = await sendRawHttpRequest(HOST, PORT, req);
        if (!res.includes('200 OK')) throw new Error('Expected 200 OK');
        if (!res.includes('jinn_uptime_seconds')) throw new Error('Missing jinn_uptime_seconds metric');
        if (!res.includes('jinn_services_running')) throw new Error('Missing jinn_services_running metric');
    });

    // Test 4: POST / (Method Not Allowed)
    await testCase('POST /api (405 Method Not Allowed)', async () => {
        const req = `POST /api HTTP/1.1\r\nHost: ${HOST}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n`;
        const res = await sendRawHttpRequest(HOST, PORT, req);
        if (!res.includes('405 Method Not Allowed')) throw new Error('Expected 405 Method Not Allowed');
        if (!res.includes('Allow: GET, HEAD')) throw new Error('Missing Allow header');
    });

    // Test 5: GET /unknown (404 Not Found)
    await testCase('GET /unknown-route (404 Not Found)', async () => {
        const req = `GET /nonexistent HTTP/1.1\r\nHost: ${HOST}\r\nConnection: close\r\n\r\n`;
        const res = await sendRawHttpRequest(HOST, PORT, req);
        if (!res.includes('404 Not Found')) throw new Error('Expected 404 Not Found');
    });

    // Test 6: Malformed HTTP request
    await testCase('Malformed Request (400 Bad Request)', async () => {
        const req = `CORRUPTED_GARBAGE_PAYLOAD\r\n\r\n`;
        const res = await sendRawHttpRequest(HOST, PORT, req);
        if (!res.includes('400 Bad Request')) throw new Error('Expected 400 Bad Request');
    });

    // Test 7: Oversized Request (>1024 bytes)
    await testCase('Oversized Request (413 Payload Too Large)', async () => {
        const padding = 'X'.repeat(1200);
        const req = `GET /?q=${padding} HTTP/1.1\r\nHost: ${HOST}\r\nConnection: close\r\n\r\n`;
        const res = await sendRawHttpRequest(HOST, PORT, req);
        if (!res.includes('413 Payload Too Large')) throw new Error('Expected 413 Payload Too Large');
    });

    // Test 8: Concurrent Client Requests
    await testCase('Concurrent Connection Handling (4 simultaneous clients)', async () => {
        const promises = [1, 2, 3, 4].map(id => {
            const req = `GET /healthz HTTP/1.1\r\nHost: ${HOST}\r\nX-Client-Id: ${id}\r\nConnection: close\r\n\r\n`;
            return sendRawHttpRequest(HOST, PORT, req);
        });
        const results = await Promise.all(promises);
        for (const res of results) {
            if (!res.includes('200 OK') || !res.includes('healthy')) {
                throw new Error('Concurrent client received invalid response');
            }
        }
    });

    console.log(`--------------------------------------------------------------------------------`);
    console.log(`  Summary: ${passed} passed, ${failed} failed (Total: ${passed + failed})`);
    console.log(`================================================================================`);

    if (failed > 0) {
        process.exit(1);
    }
}

// Standalone self-test mock server if target not currently running
if (process.argv.includes('--self-test')) {
    const server = net.createServer((socket) => {
        let buffer = Buffer.alloc(0);
        socket.on('data', (chunk) => {
            buffer = Buffer.concat([buffer, chunk]);
            if (buffer.length > 1024) {
                socket.write("HTTP/1.1 413 Payload Too Large\r\nServer: Simbad-Httpd/0.2.0\r\nConnection: close\r\n\r\n");
                socket.end();
                return;
            }
            if (buffer.includes('\r\n\r\n')) {
                const reqStr = buffer.toString('utf8');
                const firstLine = reqStr.split('\r\n')[0];
                const parts = firstLine.split(' ');
                if (parts.length < 3 || !parts[2].startsWith('HTTP/')) {
                    socket.write("HTTP/1.1 400 Bad Request\r\nServer: Simbad-Httpd/0.2.0\r\nConnection: close\r\n\r\n");
                    socket.end();
                    return;
                }
                const [method, path] = parts;
                if (method !== 'GET' && method !== 'HEAD') {
                    socket.write("HTTP/1.1 405 Method Not Allowed\r\nServer: Simbad-Httpd/0.2.0\r\nAllow: GET, HEAD\r\nConnection: close\r\n\r\n");
                    socket.end();
                    return;
                }
                if (path === '/') {
                    socket.write("HTTP/1.1 200 OK\r\nServer: Simbad-Httpd/0.2.0\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"system\":\"Jinn Microkernel\",\"manifestation\":\"Simbad Server 0.2.0\",\"status\":\"online\"}\n");
                } else if (path === '/healthz') {
                    socket.write("HTTP/1.1 200 OK\r\nServer: Simbad-Httpd/0.2.0\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nhealthy\n");
                } else if (path === '/metrics') {
                    socket.write("HTTP/1.1 200 OK\r\nServer: Simbad-Httpd/0.2.0\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\njinn_uptime_seconds 42\njinn_services_running 4\n");
                } else {
                    socket.write("HTTP/1.1 404 Not Found\r\nServer: Simbad-Httpd/0.2.0\r\nConnection: close\r\n\r\nNot Found");
                }
                socket.end();
            }
        });
    });

    server.listen(PORT, HOST, async () => {
        try {
            await runTestSuite();
        } finally {
            server.close();
        }
    });
} else {
    runTestSuite().catch(err => {
        console.error(`Error connecting to ${HOST}:${PORT}: ${err.message}`);
        console.log(`Tip: To run the external test against a local mock server, run: node tools/test_simbad_http_external.js --self-test`);
        process.exit(1);
    });
}
