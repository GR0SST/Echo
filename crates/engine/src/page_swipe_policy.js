(() => {
 try {
  if(typeof getEventListeners!=='function')return false;
  const blocking=node=>{const handlers=getEventListeners(node);return [...(handlers.wheel||[]),...(handlers.mousewheel||[])].some(handler=>!handler.passive)};
  let node=document.elementFromPoint(POINT_X,POINT_Y);
  if(!node||blocking(window))return false;
  for(let depth=0;node&&depth<100;depth++){
   if(blocking(node))return false;
   if(node.nodeType===1){
    if(node.matches('iframe,frame,embed,object,input,textarea,select,canvas,video,audio')||node.isContentEditable||node.tagName.includes('-'))return false;
    const style=getComputedStyle(node);
    if(style.overscrollBehaviorX==='none'||style.overscrollBehaviorX==='contain')return false;
    if(node.scrollWidth>node.clientWidth+1&&/(auto|scroll|overlay)/.test(style.overflowX))return false;
   }
   node=node.parentNode||node.host;
  }
  const root=document.scrollingElement;
  return !root||root.scrollWidth<=root.clientWidth+1;
 }catch{return false;}
})()
