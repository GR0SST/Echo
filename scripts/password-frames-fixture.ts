// Local native acceptance fixture. All credentials below are synthetic.
const headers = { 'content-type': 'text/html; charset=utf-8' };
for (const port of [53861, 53862, 53863, 53864, 53865]) {
  Bun.serve({ hostname: '127.0.0.1', port, fetch(request) {
    const url = new URL(request.url);
    if (port === 53861) {
      if (url.pathname === '/done') {
        const user=url.searchParams.get('username'),password=url.searchParams.get('password');
        const matched=(user==='echo-test-alpha'&&password==='test-only-alpha-53861')||(user==='echo-test-beta'&&password==='test-only-beta-53861');
        return new Response(`<h1>${matched?'Matched synthetic account '+user:'No synthetic match'}</h1>`,{headers});
      }
      return new Response(`<!doctype html><title>Echo test login</title><style>body{font:16px system-ui;padding:60px}input,button{display:block;margin:14px 0;padding:8px}</style><h1>Test credentials only</h1><form action="/done"><label>Username<input name="username" autocomplete="username"></label><label>Password<input type="password" name="password" autocomplete="current-password"></label><button>Sign in</button></form>`,{headers});
    }
    if (port === 53865) return new Response(`<!doctype html><title>Cross-origin Arc login test</title><style>body{font:16px system-ui;padding:30px}iframe{width:600px;height:440px;border:2px solid #888}</style><h1>Test credentials only</h1><iframe src="http://127.0.0.1:53861/"></iframe>`,{headers});
    if (port === 53864) {
      const target = url.searchParams.get('multi') === '1' ? 53863 : 53862;
      return new Response(`<!doctype html><title>Cross-origin login fixture</title>
        <style>body{font:16px system-ui;padding:30px}iframe{width:600px;height:400px;border:2px solid #888}</style>
        <h1>Test credentials only</h1><p id="result">Waiting for frame</p>
        <iframe src="http://127.0.0.1:${target}/"></iframe>
        <script>addEventListener('message',e=>{if(e.origin==='http://127.0.0.1:${target}' && e.source===document.querySelector('iframe').contentWindow && e.data?.probe==='echo-test-fill')document.querySelector('#result').textContent=e.data.result})</script>`, { headers });
    }
    if (url.pathname === '/outer') return new Response(`<!doctype html><title>Same-origin login fixture</title><style>body{font:16px system-ui;padding:30px}iframe{width:600px;height:400px}</style><h1>Test credentials only</h1><p id="result">Waiting for frame</p><iframe src="/"></iframe><script>addEventListener('message',e=>{if(e.origin===location.origin&&e.source===document.querySelector('iframe').contentWindow&&e.data?.probe==='echo-test-fill')document.querySelector('#result').textContent=e.data.result})</script>`,{headers});
    const accounts = port === 53862 ? [['echo-frame-test', 'synthetic-frame-only']] : [['echo-test-alpha','test-only-alpha-53861'],['echo-test-beta','test-only-beta-53861']];
    return new Response(`<!doctype html><title>Frame login fixture</title>
      <style>body{font:16px system-ui;padding:30px}input,button{display:block;margin:12px 0;padding:8px}</style>
      <form action="/done"><label>Username<input name="username" autocomplete="username"></label>
      <label>Password<input name="password" type="password" autocomplete="current-password"></label><button>Sign in</button></form>
      <script>const accounts=${JSON.stringify(accounts)};setInterval(()=>{const f=document.forms[0];const found=accounts.find(([u,p])=>f.username.value===u&&f.password.value===p);const result=found?'Matched synthetic account '+found[0]:(!f.username.value&&!f.password.value?'Empty':'No match');parent.postMessage({probe:'echo-test-fill',result},'*')},100)</script>`, { headers });
  }});
}
console.log('Synthetic iframe fixtures: http://127.0.0.1:53864/ and http://127.0.0.1:53864/?multi=1');
