import { expect, test } from "bun:test";
import { chmodSync, copyFileSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

// Exercise the real driver against tiny non-UI executable fixtures. No CEF,
// browser profile, native window or shared screen lock is involved here.
function runFixture(program: string, strict: boolean, limit = 2) {
  const root = mkdtempSync(join(tmpdir(), "echo-driver-exit-"));
  try {
    mkdirSync(join(root, "scripts"));
    const executableDir = join(root, "target/bundle/Echo.app/Contents/MacOS");
    mkdirSync(executableDir, { recursive: true });
    const runtime = join(root, "runtime");
    mkdirSync(runtime);
    const executable = join(executableDir, "Echo");
    writeFileSync(executable, `#!/bin/sh\n${program}\n`);
    chmodSync(executable, 0o755);
    const driver = join(root, "scripts/drive.sh");
    copyFileSync(join(import.meta.dir, "drive.sh"), driver);
    const started = Date.now();
    const result = Bun.spawnSync({
      cmd: ["bash", driver, "quit", String(limit)],
      env: { ...process.env, TMPDIR: `${runtime}/`, BROWSER_DRIVE_STRICT_EXIT: strict ? "1" : "0" },
      stdout: "pipe", stderr: "pipe", timeout: 6_000,
    });
    return { status: result.exitCode, elapsed: Date.now() - started, output: result.stdout.toString() + result.stderr.toString() };
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

test("strict driver preserves the application's real failure status", () => {
  const result = runFixture("echo fixture-finished; exit 7", true);
  expect(result.status).toBe(7);
  expect(result.output).toContain("fixture-finished");
  expect(result.output).toContain("application exited with status 7");
});

test("strict driver rejects a timed-out app rather than reporting a successful quit", () => {
  const result = runFixture("exec sleep 30", true, 1);
  expect(result.status).toBe(124);
  expect(result.elapsed).toBeLessThan(5_000);
  expect(result.output).toContain("timed out after 1s");
});

test("diagnostic driver mode can still collect an interrupted application's log", () => {
  const result = runFixture("echo diagnostic-fixture; exit 7", false);
  expect(result.status).toBe(0);
  expect(result.output).toContain("diagnostic-fixture");
});
