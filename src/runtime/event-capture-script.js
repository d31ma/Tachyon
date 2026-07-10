// @ts-check
/**
 * Inline, dependency-free capture script for the SSR shell. Must run before the
 * runtime module. Sets `window.__tacEventCapture = { queue, onIntent, stop }`.
 */
export const EVENT_CAPTURE_SCRIPT = `(function(){
  var TYPES=['click','submit'];
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
    q.push({type:e.type, target:handler});
    if(e.type==='click'||e.type==='submit') e.preventDefault();
    var c=window.__tacEventCapture; if(c && c.onIntent) c.onIntent();
  }
  for(var i=0;i<TYPES.length;i++) document.addEventListener(TYPES[i], rec, true);
  window.__tacEventCapture={queue:q, onIntent:null, stop:function(){ for(var i=0;i<TYPES.length;i++) document.removeEventListener(TYPES[i], rec, true); }};
})();`;
