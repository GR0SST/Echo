// One acceptance run, one framework hash. The full shutdown matrix shares one screen session.
import { mkdir } from 'node:fs/promises';
import { homedir } from 'node:os';
import { join, resolve } from 'node:path';
import { validate } from './cef-dist';
import { readdir } from 'node:fs/promises';
const root = resolve(import.meta.dir, '..');
const source = process.env.ECHO_CEF_SOURCE ?? join(homedir(), 'Documents/Bot/cef-src');
const candidateFile = Bun.file(join(source, 'candidate-path'));
const dist = await validate(process.env.ECHO_CEF_CANDIDATE ??
  (await candidateFile.exists() ? (await candidateFile.text()).trim() : join(source, 'echo-extensions-dist')));
const record = await Bun.file(join(dist, 'build.json')).json();
const evidence = join(source, 'qualification', `canonical-${new Date().toISOString().replaceAll(':', '-')}`);
await mkdir(evidence, { recursive: true });
const expected = record.framework_sha256;
if (record.schema !== 1 || !/^[a-f0-9]{64}$/.test(expected ?? '')) throw Error('Canonical CEF has no build provenance');
const bundle = resolve(process.env.ECHO_CEF_QUALIFICATION_BUNDLE ?? join(root, 'target/bundle/Echo.app'));
const env = { ...process.env, ECHO_CEF_DIST: dist, ECHO_CEF_TEST_BUNDLE: bundle, CARGO_BUILD_JOBS: '4' };
async function run(name: string, command: string[], environment = env) {
  const log = join(evidence, `${name}.log`);
  console.log(`${name}: ${log}`);
  const child = Bun.spawn(command, { cwd: root, env: environment, stdout: Bun.file(log), stderr: 'inherit' });
  const code = await child.exited;
  if (code !== 0) throw Error(`${name} failed (${code}); retain all distributions and inspect ${log}`);
  return log;
}
const report: Record<string, any> = { passed: false, dist, framework_sha256: expected, build: record, evidence };
async function requireCurrentStack() {
  const series = await Bun.file(join(root, 'patches/cef154/series.json')).json();
  const actual = (await readdir(join(root, 'patches/cef154'))).filter(path => path.endsWith('.patch')).sort();
  if (JSON.stringify(series.patches) !== JSON.stringify(record.stack.patches.map(({ path, directory }: any) => ({ path, directory }))) ||
      JSON.stringify(actual) !== JSON.stringify(record.stack.patches.map(({ path }: any) => path).sort())) throw Error('Canonical build does not contain the complete current ordered series');
  for (const patch of record.stack.patches) {
    const bytes = await Bun.file(join(root, 'patches/cef154', patch.path)).arrayBuffer();
    if (new Bun.CryptoHasher('sha256').update(bytes).digest('hex') !== patch.sha256) throw Error(`Canonical build predates ${patch.path}`);
  }
}
try {
  await requireCurrentStack();
  await run('rust-tests', ['scripts/with-cef.sh', 'cargo', 'test']);
  report.rust_tests = { passed: true };
  if (!process.env.ECHO_CEF_QUALIFICATION_BUNDLE) await run('bundle-candidate', ['scripts/with-cef.sh', 'cargo', 'run', '-p', 'bundle']);
  const frozenFramework = new Bun.CryptoHasher('sha256');
  for await (const bytes of Bun.file(join(bundle, 'Contents/Frameworks/Chromium Embedded Framework.framework/Chromium Embedded Framework')).stream()) frozenFramework.update(bytes);
  if (frozenFramework.digest('hex') !== expected) throw Error('Frozen qualification bundle contains another framework');
  report.candidate_bundle = { passed: true, path: bundle };
  const codec = JSON.parse(await Bun.file(await run('codecs', ['bun', 'scripts/check-cef-codecs.ts'])).text());
  if (!codec.passed || codec.bundledHash !== expected) throw Error('Codec result is not for the canonical framework');
  report.codecs = codec;
  const extension = JSON.parse(await Bun.file(await run('extensions', ['bun', 'scripts/check-cef-extensions.ts'], { ...env, ECHO_CEF_TEST_POPUP_ONLY: '1' })).text());
  if (!extension.passed || extension.framework_sha256 !== expected) throw Error('Extension result is not for the canonical framework');
  report.extensions = extension;
  const redacted = JSON.parse(await Bun.file(await run('extensions-redacted', ['bun', 'scripts/check-cef-extensions.ts'], { ...env, ECHO_CEF_TEST_POPUP_ONLY: '1', ECHO_CEF_TEST_REDACTED: '1' })).text());
  if (!redacted.passed || redacted.framework_sha256 !== expected) throw Error('Redacted extension metadata gate failed');
  report.extensions_redacted = redacted;
  const model = JSON.parse(await Bun.file(await run('extension-model', ['bun', 'scripts/check-cef-extension-model.ts'])).text());
  if (!model.passed || model.framework_sha256 !== expected) throw Error('GPUI extension model acceptance failed');
  report.extension_model = model;
  const real = await Bun.file(process.env.ECHO_CEF_REAL_EXTENSION_QUALIFICATION ?? join(dist, 'real-extension-qualification.json')).json();
  if (real.passed !== true || real.framework_sha256 !== expected || real.executable_sha256 !== model.executable_sha256 ||
      !['dark_reader_https', 'ubol_https', 'profile_isolation'].every(key => real.checks?.[key] === true) ||
      !Array.isArray(real.artifacts) || !real.artifacts.length) throw Error('Real Store extension HTTPS/isolation qualification is missing or belongs to another framework');
  for (const artifact of real.artifacts) if (!await Bun.file(artifact).exists()) throw Error(`Missing real-extension evidence: ${artifact}`);
  report.real_extensions = real;
  const shutdownLog = await run('shutdown', ['zsh', 'scripts/check-shutdown-matrix.sh', '--cef-dist', dist, '--bundle', bundle, '--output', join(evidence, 'shutdown'), '--require-network-exit', '--keep-going']);
  const directory = (await Bun.file(shutdownLog).text()).match(/^Evidence: (.+)$/m)?.[1];
  if (!directory) throw Error('Shutdown acceptance did not produce its evidence directory');
  const matrix = await Bun.file(join(directory, 'results.json')).json();
  const failures = await Bun.file(join(directory, 'native-failures.json')).json();
  const shutdownBundle = await Bun.file(join(directory, 'bundle.json')).json();
  if (matrix.length !== 8 || matrix.some((r: any) => !r.passed || !r.cookies_verified) || failures.length || shutdownBundle.framework_sha256 !== expected) throw Error('The complete 16-exit canonical shutdown matrix did not pass');
  report.shutdown = { passed: true, clean_exits: 16, directory, matrix };
  const pip = JSON.parse(await Bun.file(await run('pip', ['python3', 'scripts/check-cef-pip.py', '--cef-dist', dist, '--bundle', bundle, '--output', evidence])).text());
  if (!pip.passed || pip.framework_sha256 !== expected) throw Error('PiP result is not for the canonical framework');
  report.pip = pip;
  const recovery = JSON.parse(await Bun.file(await run('shutdown-recovery', ['python3', 'scripts/check-cef-recovery.py', '--cef-dist', dist, '--output', evidence])).text());
  if (!recovery.passed || recovery.framework_sha256 !== expected) throw Error('Dirty-shutdown recovery result is not for the canonical framework');
  report.shutdown_recovery = recovery;
  const browsing = JSON.parse(await Bun.file(await run('browsing-data', ['python3', 'scripts/check-cef-browsing-data.py', '--cef-dist', dist, '--output', evidence])).text());
  if (!browsing.passed || browsing.framework_sha256 !== expected) throw Error('Browsing-data result is not for the canonical framework');
  report.browsing_data = browsing;
  await requireCurrentStack();
  report.native_passed = true;
  await Bun.write(join(dist, 'qualification.json'), JSON.stringify(report, null, 2) + '\n');
  await run('publish', ['lockf', '-k', '-t', '120', '/tmp/echo-cef-build.lock', 'bun', 'scripts/cef-dist.ts', 'publish', dist, join(source, 'echo-extensions-dist')]);
  // Exercise 8ae665b only after the candidate's five native gates pass.
  const automatic = { ...process.env, CARGO_BUILD_JOBS: '4' };
  delete automatic.CEF_PATH; delete automatic.ECHO_CEF_DIST;
  await run('bundle-default', ['cargo', 'run', '-p', 'bundle'], automatic);
  const hash = new Bun.CryptoHasher('sha256');
  for await (const bytes of Bun.file(join(root, 'target/bundle/Echo.app/Contents/Frameworks/Chromium Embedded Framework.framework/Chromium Embedded Framework')).stream()) hash.update(bytes);
  if (hash.digest('hex') !== expected) throw Error('Automatic machine default did not select the qualified framework');
  report.default_bundle = { passed: true };
  report.passed = true;
  report.completed_utc = new Date().toISOString();
  await Bun.write(join(dist, 'qualification.json'), JSON.stringify(report, null, 2) + '\n');
} finally {
  await Bun.write(join(evidence, 'result.json'), JSON.stringify(report, null, 2) + '\n');
  console.log(JSON.stringify({ passed: report.passed, framework_sha256: expected, evidence }, null, 2));
}
