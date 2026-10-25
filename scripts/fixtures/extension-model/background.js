const pause = ms => new Promise(resolve=>setTimeout(resolve,ms));
async function until(fn,label){for(let i=0;i<100;i++){const value=await fn();if(value)return value;await pause(100);}throw Error(`Timeout: ${label}`);}
chrome.runtime.onMessage.addListener((message,sender,reply)=>{
 if(message==='sender-probe'){reply({tab:sender.tab});return;}
 if(message!=='run-acceptance')return;
 (async()=>{
  const checks=[],events=[];let created;
  const assert=(value,label)=>{if(!value)throw Error(label);checks.push(label);};
  const listeners={created:tab=>events.push({kind:'created',tab}),updated:(id,change,tab)=>events.push({kind:'updated',id,change,tab}),removed:(id,info)=>events.push({kind:'removed',id,info}),activated:info=>events.push({kind:'activated',info}),focus:id=>events.push({kind:'focus',id})};
  chrome.tabs.onCreated.addListener(listeners.created);chrome.tabs.onUpdated.addListener(listeners.updated);chrome.tabs.onRemoved.addListener(listeners.removed);chrome.tabs.onActivated.addListener(listeners.activated);chrome.windows.onFocusChanged.addListener(listeners.focus);
  try{
   await pause(4000);
   const [original]=await chrome.tabs.query({active:true,currentWindow:true});assert(original?.url?.startsWith('https://www.rfc-editor.org/'),'HTTPS owner resolved');
   assert((await chrome.tabs.get(original.id)).windowId===original.windowId,'get identity matches query');
   const window=await chrome.windows.getCurrent({populate:true});assert(window.id===original.windowId&&window.tabs.some(tab=>tab.id===original.id),'window projection matches owner');
   for(const url of ['javascript:void(0)','chrome://crash/']) {
    let rejected=false;try{await chrome.tabs.create({url,active:false});}catch{rejected=true;}
    assert(rejected,`Chromium rejects prohibited navigation: ${url}`);
   }
   const response=await chrome.tabs.sendMessage(original.id,{probe:true});assert(response.url===original.url,'normal tabs.sendMessage reaches HTTPS content script');
   const sender=await chrome.tabs.sendMessage(original.id,{senderProbe:true});assert(sender.tab.id===original.id&&sender.tab.windowId===original.windowId&&sender.tab.active===true,'normal content message sender carries model metadata');
   created=await chrome.tabs.create({url:original.url+'#echo-model-created',active:false,openerTabId:original.id});
   assert(created.id!==original.id&&created.windowId===original.windowId&&!created.active&&created.openerTabId===original.id,'background create reserves model identity and opener');
   await until(async()=>{try{return (await chrome.tabs.sendMessage(created.id,{probe:true})).url.includes('#echo-model-created');}catch{return false;}},'new renderer and message routing');checks.push('created renderer adopts reserved ID');
   assert((await chrome.tabs.query({active:true,currentWindow:true}))[0]?.id===original.id,'background renderer creation preserves the selected GPUI tab');
   let updated=await chrome.tabs.update(created.id,{active:true,muted:true,autoDiscardable:false,url:original.url+'#echo-model-updated'});
   assert(updated.id===created.id&&updated.active&&updated.mutedInfo.muted&&!updated.autoDiscardable,'update selects actual tab and changes metadata');
   assert((await chrome.tabs.query({active:true,currentWindow:true}))[0].id===created.id,'active query follows GPUI selection');
   await until(async()=>{try{return (await chrome.tabs.sendMessage(created.id,{probe:true})).url.includes('#echo-model-updated');}catch{return false;}},'updated navigation');
   await until(()=>events.some(e=>e.kind==='updated'&&e.id===created.id&&e.change.url===original.url+'#echo-model-updated'),'committed URL update event');checks.push('onUpdated reports the committed URL');
   const reloadEvents=events.length;
   await chrome.tabs.reload(created.id,{bypassCache:true});
   await until(()=>{
    const statuses=events.slice(reloadEvents).filter(e=>e.kind==='updated'&&e.id===created.id).map(e=>e.change.status);
    return statuses.includes('loading')&&statuses.includes('complete');
   },'reload loading and complete events');checks.push('reload emits ordered loading and complete updates');
   await until(async()=>{try{return !!await chrome.tabs.sendMessage(created.id,{probe:true});}catch{return false;}},'reload renderer');checks.push('reload retains messaging identity');
   await chrome.tabs.remove(created.id);await until(async()=>!(await chrome.tabs.query({})).some(tab=>tab.id===created.id),'remove reflected');
   assert(events.some(e=>e.kind==='created'&&e.tab.id===created.id),'onCreated delivered');
   assert(events.some(e=>e.kind==='updated'&&e.id===created.id&&e.change.mutedInfo?.muted),'onUpdated metadata delivered');
   assert(events.some(e=>e.kind==='activated'&&e.info.tabId===created.id),'onActivated delivered');
   assert(events.some(e=>e.kind==='removed'&&e.id===created.id&&e.info.windowId===original.windowId),'onRemoved delivered');
   assert(events.some(e=>e.kind==='focus'&&e.id===-1)&&events.some(e=>e.kind==='focus'&&e.id===original.windowId),'onFocusChanged follows GPUI app focus');
   return {passed:true,checks,events,original,created};
  }catch(error){if(created)try{await chrome.tabs.remove(created.id);}catch{}return {passed:false,error:String(error),checks,events};}
  finally{chrome.tabs.onCreated.removeListener(listeners.created);chrome.tabs.onUpdated.removeListener(listeners.updated);chrome.tabs.onRemoved.removeListener(listeners.removed);chrome.tabs.onActivated.removeListener(listeners.activated);chrome.windows.onFocusChanged.removeListener(listeners.focus);}
 })().then(async result=>{await chrome.storage.local.set({modelAcceptance:result});const {reportUrl}=await (await fetch(chrome.runtime.getURL('config.json'))).json();if(reportUrl)await fetch(reportUrl,{method:'POST',body:JSON.stringify({modelAcceptance:result})});reply(result);},error=>reply({passed:false,error:String(error)}));return true;
});
