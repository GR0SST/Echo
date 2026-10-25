document.querySelector('#status').textContent='Testing real GPUI model…';
const result=await chrome.runtime.sendMessage('run-acceptance');
document.querySelector('#status').textContent=JSON.stringify(result,null,2);
document.querySelector('#identity').textContent=result.passed?'MODEL ACCEPTANCE PASSED':result.error;
document.title=result.passed?'MODEL ACCEPTANCE PASSED':'MODEL ACCEPTANCE FAILED';
