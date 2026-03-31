// scripts/test.js
// Finds a free OS port, sets TEST_PORT, spawns `playwright test`.
// Cross-platform — no bash required.
const net = require('net');
const { spawn } = require('child_process');

const server = net.createServer();
server.listen(0, '127.0.0.1', () => {
  const port = server.address().port;
  server.close(() => {
    const proc = spawn(
      'npx',
      ['playwright', 'test', ...process.argv.slice(2)],
      {
        env: { ...process.env, TEST_PORT: String(port) },
        stdio: 'inherit',
        shell: true,
      }
    );
    proc.on('close', code => process.exit(code ?? 0));
  });
});
