// Bind packaging and qualification to the exact stack and compiled framework.
import { join } from 'node:path';
import { version, validate } from './cef-dist';
async function sha(path: string) {
  const hash = new Bun.CryptoHasher('sha256');
  for await (const bytes of Bun.file(path).stream()) hash.update(bytes);
  return hash.digest('hex');
}
async function head(directory: string) {
  const child = Bun.spawn(['git', '-C', directory, 'rev-parse', 'HEAD'], { stdout: 'pipe', stderr: 'pipe' });
  const text = await new Response(child.stdout).text();
  if (await child.exited !== 0) throw Error(`Cannot read source pin: ${directory}`);
  return text.trim();
}
const binary = (source: string) => join(source, 'out/Release_GN_arm64/Chromium Embedded Framework.framework/Chromium Embedded Framework');
const [mode, path, source, ...args] = Bun.argv.slice(2);
try {
  if (mode === 'write' && args.length === 5) {
    const [tools, stackPath, gn, started, elapsed] = args;
    const record = { schema: 1, cef_version: version, chromium_commit: await head(source), cef_commit: await head(join(source, 'cef')),
      depot_tools_commit: await head(tools), gn_defines: gn,
      gn_args: await Bun.file(join(source, 'out/Release_GN_arm64/args.gn')).text(),
      started_utc: started, build_seconds: Number(elapsed),
      framework_sha256: await sha(binary(source)), stack: await Bun.file(stackPath).json() };
    await Bun.write(path, JSON.stringify(record, null, 2) + '\n');
  } else if (mode === 'verify' && args.length === 2) {
    const [stackPath, gn] = args, record = await Bun.file(path).json(), stack = await Bun.file(stackPath).json();
    if (record.schema !== 1 || record.cef_version !== version || record.gn_defines !== gn ||
        record.gn_args !== await Bun.file(join(source, 'out/Release_GN_arm64/args.gn')).text() ||
        record.stack.stack_sha256 !== stack.stack_sha256 || record.stack.source_sha256 !== stack.source_sha256 ||
        record.framework_sha256 !== await sha(binary(source))) throw Error('Compiled CEF does not match the current stack/arguments; run scripts/build-cef.sh release');
    console.log(record.framework_sha256);
  } else if (mode === 'stage' && args.length === 0) {
    const record = await Bun.file(path).json(), dist = await validate(source);
    if (record.framework_sha256 !== await sha(join(dist, 'Chromium Embedded Framework.framework/Chromium Embedded Framework'))) throw Error('Packaged framework differs from the qualified build input');
    const archive = await Bun.file(join(dist, 'archive.json')).json();
    await Bun.write(join(dist, 'build.json'), JSON.stringify({ ...record, archive, staged_utc: new Date().toISOString() }, null, 2) + '\n');
  } else throw Error('Usage: cef-build-record.ts write <record> <src> <tools> <stack> <gn> <started> <seconds> | verify <record> <src> <stack> <gn> | stage <record> <dist>');
} catch (error) { console.error(error instanceof Error ? error.message : error); process.exitCode = 1; }
