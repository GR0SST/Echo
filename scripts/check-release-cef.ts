// A release must use the published and qualified canonical distribution.
import { join } from "node:path";
import { realpath } from "node:fs/promises";
import { validate } from "./cef-dist";
const path = process.platform === "darwin" ? await validate(process.argv[2]) : await realpath(process.argv[2]);
const runtime = process.platform === "darwin" ? "Chromium Embedded Framework.framework/Chromium Embedded Framework" : process.platform === "win32" ? "libcef.dll" : "libcef.so";
const header = await Bun.file(join(path, "include/cef_version.h")).text();
if (!header.includes('#define CEF_VERSION "154.0.34+g14c5a08+chromium-154.0.8037.98"')) throw new Error("CEF version mismatch");
const build = await Bun.file(join(path, "build.json")).json();
const qualification = await Bun.file(join(path, "qualification.json")).json();
const hash = new Bun.CryptoHasher("sha256");
for await (const chunk of Bun.file(join(path, runtime)).stream()) hash.update(chunk);
const digest = hash.digest("hex");
if (!qualification.passed || await realpath(qualification.dist) !== path || build.framework_sha256 !== digest || qualification.framework_sha256 !== digest) {
  throw new Error("Release CEF does not match its canonical qualification receipt");
}
console.log(path);
