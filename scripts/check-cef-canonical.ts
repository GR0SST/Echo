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
const evidence = join(homedir(), 'Library/Caches/EchoCEFQualification', `canonical-${new Date().toISOString().replaceAll(':', '-')}`);
await mkdir(evidence, { recursive: true });
const expected = record.framework_sha256;
if (record.schema !== 1 || !/^[a-f0-9]{64}$/.test(expected ?? '')) throw Error('Canonical CEF has no build provenance');
const bundle = resolve(process.env.ECHO_CEF_QUALIFICATION_BUNDLE ?? join(root, 'target/bundle/Echo.app'));
const shutdownBundle = resolve(process.env.ECHO_CEF_SHUTDOWN_BUNDLE ?? bundle);
const env = { ...process.env, ECHO_CEF_DIST: dist, ECHO_CEF_TEST_BUNDLE: bundle, CARGO_BUILD_JOBS: '4' };
let nativeUntil = 0;
async function run(name: string, command: string[], environment = env) {
  const log = join(evidence, `${name}.log`);
  console.log(`${name}: ${log}`);
  if (nativeUntil) {
    const remaining = Math.floor((nativeUntil - Date.now()) / 1000);
    if (remaining < 5) throw Error('The native screen session has expired');
    command = ['gtimeout', '-k', '2', String(remaining - 2), ...command];
  }
  const child = Bun.spawn(command, { cwd: root, env: environment, stdout: Bun.file(log), stderr: 'inherit' });
  const code = await child.exited;
  if (code !== 0) throw Error(`${name} failed (${code}); retain all distributions and inspect ${log}`);
  return log;
}
const report: Record<string, any> = { passed: false, dist, framework_sha256: expected, build: record, evidence };
const screenDeadline = process.env.ECHO_CEF_SCREEN_DEADLINE ? Date.parse(process.env.ECHO_CEF_SCREEN_DEADLINE) : Infinity;
if (Number.isNaN(screenDeadline)) throw Error('Invalid ECHO_CEF_SCREEN_DEADLINE');
function requireScreenTime() {
  if (Date.now() + 700_000 >= screenDeadline) throw Error('Insufficient time before the screen-work cutoff; candidate remains unpublished');
}
async function oneScreenSession(work: () => Promise<void>) {
  requireScreenTime();
  const ready = join(evidence, 'native-screen-ready');
  const release = join(evidence, 'native-screen-release');
  const holder = Bun.spawn(['lockf', '-k', '/tmp/echo-screen.lock', 'gtimeout', '-k', '5', '690', 'zsh', '-c',
    'touch /tmp/echo-screen-long; trap "rm -f /tmp/echo-screen-long" EXIT; touch "$1"; while [[ ! -f "$2" ]]; do sleep 1; done',
    '--', ready, release], { stdout: 'inherit', stderr: 'inherit' });
  try {
    while (!await Bun.file(ready).exists()) {
      requireScreenTime();
      if (holder.exitCode !== null) throw Error('The native screen session ended before acquisition');
      await Bun.sleep(100);
    }
    requireScreenTime();
    console.log(`native-screen-session: ${ready}`);
    nativeUntil = Math.min(Date.now() + 680_000, screenDeadline);
    await run('screen-preflight', ['python3', 'scripts/check-shutdown-preflight.py']);
    await work();
    if (holder.exitCode !== null) throw Error('The bounded native screen session expired');
  } finally {
    nativeUntil = 0;
    await Bun.write(release, 'release\n');
    if (!await Bun.file(ready).exists()) holder.kill();
    await holder.exited;
  }
}
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
  const shutdownFramework = new Bun.CryptoHasher('sha256');
  for await (const bytes of Bun.file(join(shutdownBundle, 'Contents/Frameworks/Chromium Embedded Framework.framework/Chromium Embedded Framework')).stream()) shutdownFramework.update(bytes);
  if (shutdownFramework.digest('hex') !== expected) throw Error('Rebased shutdown bundle contains another framework');
  const shutdownExecutable = new Bun.CryptoHasher('sha256');
  for await (const bytes of Bun.file(join(shutdownBundle, 'Contents/MacOS/Echo')).stream()) shutdownExecutable.update(bytes);
  report.shutdown_bundle = { path: shutdownBundle, executable_sha256: shutdownExecutable.digest('hex'), framework_sha256: expected };
  let resumed = false;
  if (process.env.ECHO_CEF_RESUME_QUALIFICATION) {
    const prior = await Bun.file(process.env.ECHO_CEF_RESUME_QUALIFICATION).json();
    const executable = new Bun.CryptoHasher('sha256');
    for await (const bytes of Bun.file(join(bundle, 'Contents/MacOS/Echo')).stream()) executable.update(bytes);
    const executableHash = executable.digest('hex');
    if (prior.dist !== dist || prior.framework_sha256 !== expected || prior.candidate_bundle?.path !== bundle ||
        !prior.codecs?.passed || prior.codecs.bundledHash !== expected || prior.codecs.executableHash !== executableHash ||
        !prior.extensions?.passed || prior.extensions.framework_sha256 !== expected ||
        !prior.extensions_redacted?.passed || prior.extensions_redacted.framework_sha256 !== expected ||
        !prior.extension_model?.passed || prior.extension_model.framework_sha256 !== expected ||
        prior.extension_model.executable_sha256 !== executableHash) throw Error('Cannot resume native gates from another or incomplete frozen build');
    for (const key of ['codecs', 'extensions', 'extensions_redacted', 'extension_model']) report[key] = prior[key];
    report.resumed_native_evidence = prior.evidence;
    resumed = true;
  }
  await oneScreenSession(async () => {
  if (!resumed) {
  const codec = JSON.parse(await Bun.file(await run('codecs', ['bun', 'scripts/check-cef-codecs.ts', '--locked'])).text());
  if (!codec.passed || codec.bundledHash !== expected) throw Error('Codec result is not for the canonical framework');
  report.codecs = codec;
  const extension = JSON.parse(await Bun.file(await run('extensions', ['bun', 'scripts/check-cef-extensions.ts', '--locked'], { ...env, ECHO_CEF_TEST_POPUP_ONLY: '1' })).text());
  if (!extension.passed || extension.framework_sha256 !== expected) throw Error('Extension result is not for the canonical framework');
  report.extensions = extension;
  const redacted = JSON.parse(await Bun.file(await run('extensions-redacted', ['bun', 'scripts/check-cef-extensions.ts', '--locked'], { ...env, ECHO_CEF_TEST_POPUP_ONLY: '1', ECHO_CEF_TEST_REDACTED: '1' })).text());
  if (!redacted.passed || redacted.framework_sha256 !== expected) throw Error('Redacted extension metadata gate failed');
  report.extensions_redacted = redacted;
  const model = JSON.parse(await Bun.file(await run('extension-model', ['bun', 'scripts/check-cef-extension-model.ts', '--locked'])).text());
  if (!model.passed || model.framework_sha256 !== expected) throw Error('GPUI extension model acceptance failed');
  report.extension_model = model;
  }
  const model = report.extension_model;
  if (process.env.ECHO_CEF_DEVELOPER_RUNNER && !resumed) {
    const command = JSON.parse(process.env.ECHO_CEF_DEVELOPER_RUNNER);
    if (!Array.isArray(command) || !command.length || command.some(part => typeof part !== 'string')) throw Error('Developer runner must be a JSON argv array');
    await run('developer-ui', command, { ...env, ECHO_CEF_NATIVE_SESSION: '1' });
  }
  const real = await Bun.file(process.env.ECHO_CEF_REAL_EXTENSION_QUALIFICATION ?? join(dist, 'real-extension-qualification.json')).json();
  if (real.passed !== true || real.framework_sha256 !== expected || real.executable_sha256 !== model.executable_sha256 ||
      !['dark_reader_https', 'ubol_https', 'profile_isolation'].every(key => real.checks?.[key] === true) ||
      !Array.isArray(real.artifacts) || !real.artifacts.length) throw Error('Real Store extension HTTPS/isolation qualification is missing or belongs to another framework');
  for (const artifact of real.artifacts) if (!await Bun.file(artifact).exists()) throw Error(`Missing real-extension evidence: ${artifact}`);
  report.real_extensions = real;
  const developer = await Bun.file(process.env.ECHO_CEF_DEVELOPER_QUALIFICATION ?? join(dist, 'developer-qualification.json')).json();
  if (developer.passed !== true || developer.framework_sha256 !== expected || developer.executable_sha256 !== model.executable_sha256 ||
      !['load_unpacked', 'update', 'worker_inspection', 'permissions', 'pack', 'overwrite_confirmation', 'profile_isolation'].every(key => developer.checks?.[key] === true) ||
      !Array.isArray(developer.artifacts) || !developer.artifacts.length) throw Error('Developer tooling qualification is missing or belongs to another framework/executable');
  for (const artifact of developer.artifacts) if (!await Bun.file(artifact).exists()) throw Error(`Missing developer-tool evidence: ${artifact}`);
  report.extension_developer = developer;
  const pip = JSON.parse(await Bun.file(await run('pip', ['python3', 'scripts/check-cef-pip.py', '--locked', '--cef-dist', dist, '--bundle', bundle, '--output', evidence])).text());
  if (!pip.passed || pip.framework_sha256 !== expected) throw Error('PiP result is not for the canonical framework');
  report.pip = pip;
  const recovery = JSON.parse(await Bun.file(await run('shutdown-recovery', ['python3', 'scripts/check-cef-recovery.py', '--locked', '--cef-dist', dist, '--bundle', shutdownBundle, '--output', evidence])).text());
  if (!recovery.passed || recovery.framework_sha256 !== expected || recovery.bundle !== shutdownBundle || recovery.executable_sha256 !== report.shutdown_bundle.executable_sha256) throw Error('Dirty-shutdown recovery result is not for the rebased canonical bundle');
  report.shutdown_recovery = recovery;
  const browsing = JSON.parse(await Bun.file(await run('browsing-data', ['python3', 'scripts/check-cef-browsing-data.py', '--locked', '--cef-dist', dist, '--bundle', shutdownBundle, '--output', evidence])).text());
  if (!browsing.passed || browsing.framework_sha256 !== expected || browsing.fixture_bundle !== shutdownBundle || browsing.fixture_executable_sha256 !== report.shutdown_bundle.executable_sha256) throw Error('Browsing-data result is not for the rebased canonical bundle');
  report.browsing_data = browsing;
  });
  requireScreenTime();
  const shutdownLog = await run('shutdown', ['zsh', 'scripts/check-shutdown-matrix.sh', '--cef-dist', dist, '--bundle', shutdownBundle, '--output', join(evidence, 'shutdown'), '--require-network-exit', '--keep-going']);
  const directory = (await Bun.file(shutdownLog).text()).match(/^Evidence: (.+)$/m)?.[1];
  if (!directory) throw Error('Shutdown acceptance did not produce its evidence directory');
  const matrix = await Bun.file(join(directory, 'results.json')).json();
  const failures = await Bun.file(join(directory, 'native-failures.json')).json();
  const matrixBundle = await Bun.file(join(directory, 'bundle.json')).json();
  if (matrix.length !== 8 || matrix.some((r: any) => !r.passed || !r.cookies_verified) || failures.length || matrixBundle.framework_sha256 !== expected || matrixBundle.source !== shutdownBundle || matrixBundle.main_sha256 !== report.shutdown_bundle.executable_sha256) throw Error('The complete 16-exit rebased canonical shutdown matrix did not pass');
  report.shutdown = { passed: true, clean_exits: 16, directory, matrix };
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
