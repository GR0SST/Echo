import { test, expect } from 'bun:test';
import { mkdtempSync, mkdirSync, writeFileSync, chmodSync, symlinkSync, rmSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

// Run the actual installer policy against disposable paths. Compilation is a
// sentinel; refusal must happen before CEF validation, builds or replacement.
function fixture(kind: 'absent' | 'unmanaged' | 'symlink' | 'wrong-id' | 'managed', update = false) {
  const root = mkdtempSync(join(tmpdir(), 'echo-installer-'));
  try {
    const app = join(root, 'Echo.app');
    mkdirSync(join(root, 'scripts')); mkdirSync(join(root, 'bin'));
    if (kind === 'symlink') symlinkSync(root, app);
    else if (kind !== 'absent') {
      mkdirSync(join(app, 'Contents/Resources'), { recursive: true });
      writeFileSync(join(app, 'Contents/Info.plist'), `<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>${kind === 'wrong-id' ? 'other.browser' : 'dev.echo.browser'}</string></dict></plist>`);
      if (kind !== 'unmanaged') writeFileSync(join(app, 'Contents/Resources/echo-dogfood-install.txt'), 'echo-dogfood-installer-v1:dev.echo.browser\n');
    }
    for (const [command, body] of Object.entries({uname:'echo Darwin', pgrep:'exit 1', git:'echo 123', bun:'echo /tmp/qualified-fixture', cargo:'echo BUILD-SENTINEL; exit 99'})) {
      const path = join(root, 'bin', command); writeFileSync(path, `#!/bin/sh\n${body}\n`); chmodSync(path, 0o755);
    }
    const script = (readFileSync(join(import.meta.dir, 'install-dogfood.sh'), 'utf8')).replace('APP=/Applications/Echo.app', `APP='${app}'`);
    const path = join(root, 'scripts/install-dogfood.sh'); writeFileSync(path, script);
    const result = Bun.spawnSync(['bash', path, ...(update ? ['--update'] : [])], { env: { ...process.env, ECHO_DOGFOOD_LOCKED:'1', PATH:`${root}/bin:${process.env.PATH}` }, stdout:'pipe', stderr:'pipe', timeout:5000 });
    return { status:result.exitCode, text:result.stdout.toString()+result.stderr.toString() };
  } finally { rmSync(root, { recursive:true, force:true }); }
}
test('refuses unmanaged installs, symlinks and mismatched identities before building', () => {
  for (const kind of ['unmanaged','symlink','wrong-id'] as const) {
    const result = fixture(kind); expect(result.status).toBe(1); expect(result.text).toContain('Refusing'); expect(result.text).not.toContain('BUILD-SENTINEL');
  }
});
test('update requires an existing managed installation', () => {
  const result = fixture('absent', true); expect(result.status).toBe(1); expect(result.text).toContain('No managed'); expect(result.text).not.toContain('BUILD-SENTINEL');
});
test('owned installs and fresh installs may reach the build step', () => {
  for (const [kind, update] of [['managed',true],['absent',false]] as const) {
    const result = fixture(kind, update); expect(result.status).toBe(99); expect(result.text).toContain('BUILD-SENTINEL');
  }
});
