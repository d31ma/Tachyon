// @ts-check
/**
 * Inline, dependency-free capture script for the SSR shell. Must run before the
 * runtime module. Sets `window.__tacEventCapture = { queue, onIntent, stop }`.
 */
export const EVENT_CAPTURE_SCRIPT = `(function(){
  var types={};
  var q=[];
  function tacTarget(el){
    for(var n=el; n && n!==document.body && n!==document; n=n.parentElement){
      if(n.hasAttribute('href') && (n.tagName==='A' || n.tagName.indexOf('-')!==-1) && !n.hasAttribute('disabled')) return n;
      var a=n.attributes; for(var i=0;i<a.length;i++){ if(a[i].name.indexOf('data-tac-on-')===0) return n; }
    }
    return null;
  }
  function rec(e){
    var t=e.target; if(!t || t.nodeType!==1) return;
    var handler=tacTarget(t); if(!handler) return;
    var init={bubbles:e.bubbles,composed:e.composed,cancelable:e.cancelable,detail:e.detail,key:e.key,code:e.code,location:e.location,repeat:e.repeat,isComposing:e.isComposing,ctrlKey:e.ctrlKey,shiftKey:e.shiftKey,altKey:e.altKey,metaKey:e.metaKey,button:e.button,buttons:e.buttons,clientX:e.clientX,clientY:e.clientY,screenX:e.screenX,screenY:e.screenY,pointerId:e.pointerId,pointerType:e.pointerType,isPrimary:e.isPrimary,pressure:e.pressure,width:e.width,height:e.height,relatedTarget:e.relatedTarget,submitter:e.submitter,data:e.data,inputType:e.inputType};
    var record={type:e.type,target:handler,kind:e.constructor&&e.constructor.name,init:init}; q.push(record);
    e.stopImmediatePropagation(); e.stopPropagation();
    if(e.type==='click'||e.type==='submit') e.preventDefault();
    var c=window.__tacEventCapture; if(c && c.onIntent) c.onIntent(record);
  }
  function listen(type){ if(!type||types[type]) return; types[type]=1; document.addEventListener(type,rec,true); }
  function discover(root){
    if(!root||root.nodeType!==1) return;
    var nodes=[root], all=root.getElementsByTagName('*');
    for(var j=0;j<all.length;j++) nodes.push(all[j]);
    for(var n=0;n<nodes.length;n++){
      var a=nodes[n].attributes;
      for(var i=0;i<a.length;i++) if(a[i].name.indexOf('data-tac-on-')===0) listen(a[i].name.slice(12).replace(/__/g,':'));
    }
  }
  listen('click'); listen('submit'); listen('keydown');
  var observer=new MutationObserver(function(records){
    for(var i=0;i<records.length;i++){
      var record=records[i];
      if(record.type==='attributes') discover(record.target);
      for(var j=0;j<record.addedNodes.length;j++) discover(record.addedNodes[j]);
    }
  });
  observer.observe(document.documentElement,{subtree:true,childList:true,attributes:true});
  window.__tacEventCapture={queue:q, onIntent:null, stop:function(){ observer.disconnect(); for(var type in types) document.removeEventListener(type,rec,true); }};
})();`;
