// Run separately with Bun. Disposable fixture only; do not expose remotely.
const port = Number(Bun.env.ECHO_PROBE_PORT ?? "18473");
const block = new Uint8Array(128 * 1024).fill(0x45);
Bun.serve({
  hostname: "127.0.0.1", port, idleTimeout: 0,
  fetch(request) {
    const url = new URL(request.url);
    const match = /^\/download\/(echo-download-probe-\d+-\d+\.bin)$/.exec(url.pathname);
    if (!match || request.method !== "GET" || url.search) return new Response("Fixture only", { status: 404 });
    const filename = match[1];
    console.log(JSON.stringify({ event: "request", filename, time: performance.now() }));
    let chunks = 0;
    let timer: ReturnType<typeof setTimeout>;
    const body = new ReadableStream<Uint8Array>({
      start(controller) {
        const send = () => {
          controller.enqueue(block);
          chunks++;
          console.log(JSON.stringify({ event: "body", filename, bytes: chunks * block.length, time: performance.now() }));
          if (chunks === 256) {
            controller.close();
            console.log(JSON.stringify({ event: "end", filename, time: performance.now() }));
          } else timer = setTimeout(send, 250);
        };
        timer = setTimeout(send, 250);
      },
      cancel() { clearTimeout(timer); console.log(JSON.stringify({ event: "disconnect", filename, chunks, time: performance.now() })); },
    });
    return new Response(body, { headers: {
      "Content-Type": "application/octet-stream",
      "Content-Length": String(32 * 1024 * 1024),
      "Content-Disposition": `attachment; filename="${filename}"`,
      "Cache-Control": "no-store",
    } });
  },
});
console.log(`fixture origin http://127.0.0.1:${port}`);
