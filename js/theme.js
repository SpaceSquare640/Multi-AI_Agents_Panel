/* ============================================================================
   Theme
   ----------------------------------------------------------------------------
   Three states, matching the app's Settings screen:

     "system"  no data-theme attribute; prefers-color-scheme decides
     "light"   data-theme="light" forces light even when the OS is dark
     "dark"    data-theme="dark"  forces dark  even when the OS is light

   The attribute is written on <html> as early as possible so the page never
   paints one theme and then flips. Preference is persisted per browser; in
   the real app this maps to the stored setting instead.
   ========================================================================== */

(function () {
  var KEY = 'maap.theme';
  var ORDER = ['system', 'light', 'dark'];

  function read() {
    try {
      var v = localStorage.getItem(KEY);
      return ORDER.indexOf(v) === -1 ? 'system' : v;
    } catch (e) {
      // Private windows and blocked site data both throw here. Falling back
      // to "system" is correct: it is the state that needs no storage.
      return 'system';
    }
  }

  function write(v) {
    try { localStorage.setItem(KEY, v); } catch (e) { /* not fatal */ }
  }

  function apply(v) {
    var root = document.documentElement;
    if (v === 'system') root.removeAttribute('data-theme');
    else root.setAttribute('data-theme', v);
    root.setAttribute('data-theme-pref', v);
    document.dispatchEvent(new CustomEvent('themechange', { detail: { theme: v } }));
  }

  // Applied immediately, before DOMContentLoaded, to avoid a flash.
  apply(read());

  var Theme = {
    get: read,
    set: function (v) { write(v); apply(v); },
    /* Cycles system -> light -> dark -> system. Used by the titlebar button;
       Settings offers the three states explicitly instead, because a cycling
       control is not discoverable enough to be the only way to change it. */
    cycle: function () {
      var next = ORDER[(ORDER.indexOf(read()) + 1) % ORDER.length];
      Theme.set(next);
      return next;
    },
    /* What is actually on screen right now, which is not the same as the
       preference when the preference is "system". */
    resolved: function () {
      var pref = read();
      if (pref !== 'system') return pref;
      return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
    }
  };

  window.Theme = Theme;

  document.addEventListener('DOMContentLoaded', function () {
    document.querySelectorAll('[data-action="theme-cycle"]').forEach(function (btn) {
      btn.addEventListener('click', function () {
        var next = Theme.cycle();
        btn.setAttribute('aria-label', 'Theme: ' + next + '. Click to change.');
      });
    });

    document.querySelectorAll('[data-action="theme-set"]').forEach(function (el) {
      el.addEventListener('click', function () { Theme.set(el.dataset.theme); });
    });
  });
})();
