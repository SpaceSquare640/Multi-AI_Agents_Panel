/* ============================================================================
   Icon sprite
   ----------------------------------------------------------------------------
   One inline sprite, injected into every page, referenced as
   <svg class="icon"><use href="#i-name"/></svg>.

   Inline rather than an external file because <use> pointing at another
   document is not supported in browsers, and inline rather than duplicated per
   page because a second copy is how two pages end up with two different
   versions of the same icon.

   House rules: one family, 20x20 viewBox, 1.5 stroke, round caps and joins,
   no fills. Never an emoji — emoji render differently on every platform and
   cannot be recoloured by a token.
   ========================================================================== */

(function () {
  var ICONS = {
    // navigation
    'i-chat':        '<path d="M3 5.5A1.5 1.5 0 0 1 4.5 4h11A1.5 1.5 0 0 1 17 5.5v7a1.5 1.5 0 0 1-1.5 1.5H8l-4 3v-3H4.5A1.5 1.5 0 0 1 3 12.5z"/>',
    'i-notes':       '<path d="M5 3h7l3 3v11a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1z"/><path d="M12 3v3h3M7 10h6M7 13h4"/>',
    'i-models':      '<path d="M10 3 3 6.5 10 10l7-3.5z"/><path d="M3 10.5 10 14l7-3.5M3 14 10 17.5 17 14"/>',
    'i-skills':      '<path d="M12.5 3a4 4 0 0 0-3.2 6.4L3 15.7 4.3 17l6.3-6.3A4 4 0 1 0 12.5 3z"/>',
    'i-search':      '<circle cx="9" cy="9" r="5"/><path d="M13 13l4 4"/>',
    'i-game':        '<rect x="2.5" y="6.5" width="15" height="8" rx="3"/><path d="M6 10.5h2.5M7.25 9.25v2.5M13 10h.01M15 11.5h.01"/>',
    'i-usage':       '<path d="M3 17V9M8 17V4M13 17v-5M18 17V7"/>',
    'i-settings':    '<circle cx="10" cy="10" r="2.5"/><path d="M10 2.5v2M10 15.5v2M17.5 10h-2M4.5 10h-2M15.3 4.7l-1.4 1.4M6.1 13.9l-1.4 1.4M15.3 15.3l-1.4-1.4M6.1 6.1 4.7 4.7"/>',
    'i-help':        '<circle cx="10" cy="10" r="7"/><path d="M8 8a2 2 0 1 1 2.7 1.9c-.4.2-.7.6-.7 1.1v.5M10 14h.01"/>',

    // shell
    'i-panel-left':  '<rect x="3" y="4" width="14" height="12" rx="2"/><path d="M8 4v12"/>',
    'i-panel-right': '<rect x="3" y="4" width="14" height="12" rx="2"/><path d="M12 4v12"/>',
    'i-theme':       '<circle cx="10" cy="10" r="6"/><path d="M10 4v12"/><path d="M10 4a6 6 0 0 1 0 12z" fill="currentColor" stroke="none"/>',

    // actions
    'i-plus':        '<path d="M10 4v12M4 10h12"/>',
    'i-close':       '<path d="M5 5l10 10M15 5 5 15"/>',
    'i-check':       '<path d="m4 10.5 4 4 8-9"/>',
    'i-trash':       '<path d="M4 6h12M8 6V4.5A.5.5 0 0 1 8.5 4h3a.5.5 0 0 1 .5.5V6M6 6l.7 10a1 1 0 0 0 1 1h4.6a1 1 0 0 0 1-1L14 6"/>',
    'i-send':        '<path d="M3 10 17 3l-5 14-2.5-5.5z"/><path d="m9.5 11.5 7.5-8.5"/>',
    'i-copy':        '<rect x="7" y="7" width="10" height="10" rx="2"/><path d="M13 7V5a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v6a2 2 0 0 0 2 2h2"/>',
    'i-external':    '<path d="M11 4h5v5M16 4l-7 7"/><path d="M15 12v3a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2h3"/>',

    // meaning
    'i-shield':      '<path d="M10 2.5 4 5v4.5c0 3.6 2.4 6.7 6 8 3.6-1.3 6-4.4 6-8V5z"/><path d="m7.5 10 1.8 1.8 3.4-3.6"/>',
    'i-shield-alert':'<path d="M10 2.5 4 5v4.5c0 3.6 2.4 6.7 6 8 3.6-1.3 6-4.4 6-8V5z"/><path d="M10 7v3.5M10 13h.01"/>',
    'i-alert':       '<path d="M10 3.5 2.8 16h14.4z"/><path d="M10 8v3.5M10 14h.01"/>',
    'i-info':        '<circle cx="10" cy="10" r="7"/><path d="M10 9.5v4M10 6.5h.01"/>',
    'i-folder':      '<path d="M3 6a1 1 0 0 1 1-1h3.6l1.6 2H16a1 1 0 0 1 1 1v7a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1z"/>',
    'i-key':         '<circle cx="6.5" cy="10" r="3"/><path d="M9.5 10H17M14.5 10v3M17 10v2.5"/>',
    'i-local':       '<rect x="3" y="4" width="14" height="9" rx="1.5"/><path d="M6 16.5h8"/>',
    'i-cloud':       '<path d="M6.5 15a3.5 3.5 0 0 1-.4-6.98A4.5 4.5 0 0 1 15 8.6 3.2 3.2 0 0 1 14.5 15z"/>'
  };

  function inject() {
    if (document.getElementById('icon-sprite')) return;
    var defs = '';
    Object.keys(ICONS).forEach(function (id) {
      defs += '<g id="' + id + '">' + ICONS[id] + '</g>';
    });
    var svg = document.createElement('div');
    svg.innerHTML =
      '<svg id="icon-sprite" width="0" height="0" aria-hidden="true" focusable="false" ' +
           'style="position:absolute"><defs>' + defs + '</defs></svg>';
    document.body.insertBefore(svg.firstChild, document.body.firstChild);
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', inject);
  } else {
    inject();
  }
})();
