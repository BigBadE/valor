(function(){
  function setup(){
    var address = document.getElementById('address');
    var btnGo = document.getElementById('go');
    var btnBack = document.getElementById('back');
    var btnForward = document.getElementById('forward');
    var content = document.querySelector('.content');

    function navigate(){
      var url = (address && address.value ? String(address.value) : '').trim();
      if (!url) return;
      if (typeof globalThis.chromeHost !== 'undefined' && chromeHost && typeof chromeHost.navigate === 'function') {
        try { chromeHost.navigate(url); } catch (e) { console.log('chromeHost.navigate failed:', e); }
      } else {
        console.log('navigate (no host):', url);
      }
    }

    function updateNavReady(){
      var hasDest = !!(address && String(address.value || '').trim());
      if (document && document.body && document.body.classList) {
        document.body.classList.toggle('nav-ready', hasDest);
      }
    }

    if (btnGo) btnGo.addEventListener('click', navigate);
    if (address) {
      address.addEventListener('keydown', function(e){ if (e && e.key === 'Enter') navigate(); });
      address.addEventListener('input', updateNavReady);
    }
    if (btnBack) btnBack.addEventListener('click', function(){ try { if (globalThis.chromeHost && chromeHost.back) chromeHost.back(); } catch(_){} });
    if (btnForward) btnForward.addEventListener('click', function(){ try { if (globalThis.chromeHost && chromeHost.forward) chromeHost.forward(); } catch(_){} });
    if (content) content.addEventListener('click', function(){
      // Only navigate when we have a destination ready
      if (document && document.body && document.body.classList && document.body.classList.contains('nav-ready')) {
        navigate();
      }
    });

    // Initial state
    updateNavReady();
  }

  if (document && typeof document.addEventListener === 'function') {
    document.addEventListener('DOMContentLoaded', setup);
  } else {
    // Fallback if our minimal prelude lacks DOMContentLoaded
    setTimeout(setup, 0);
  }
})();