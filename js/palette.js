/* ============================================================================
   Command palette
   ----------------------------------------------------------------------------
   The second navigation axis. Every destination and most actions are reachable
   from here, which is what lets the rail stay short without anything becoming
   unreachable.

   Behaviour worth being explicit about:

   - Subsequence matching, not substring. Typing "nsc" finds "New solo
     session"; requiring a contiguous substring would make abbreviations
     useless, which is most of why a palette is faster than a menu.
   - Matches are ranked, not just filtered. A hit at a word boundary beats one
     mid-word, and a shorter name beats a longer one at equal quality, so the
     obvious answer lands under the cursor rather than somewhere in the list.
   - The list is a listbox and the input keeps focus throughout. Arrow keys
     move aria-activedescendant instead of moving focus, so a screen reader
     announces the highlighted row while typing continues to work.
   - Focus returns to whatever opened it on close. A palette that drops focus
     to the top of the document leaves keyboard users stranded.
   ========================================================================== */

(function () {

  function subsequenceScore(needle, haystack) {
    /* Returns a score, or -1 when the needle is not a subsequence at all.
       Higher is better. Also returns the matched indices so the caller can
       highlight them. */
    var n = needle.toLowerCase();
    var h = haystack.toLowerCase();
    if (!n) return { score: 0, idx: [] };

    var idx = [];
    var hi = 0;
    var score = 0;
    var streak = 0;

    for (var ni = 0; ni < n.length; ni++) {
      var ch = n[ni];
      var found = -1;
      while (hi < h.length) {
        if (h[hi] === ch) { found = hi; break; }
        hi++;
      }
      if (found === -1) return null;

      // A character right after a space, hyphen or slash is the start of a
      // word, which is what people actually abbreviate by.
      var prev = found > 0 ? h[found - 1] : ' ';
      var atBoundary = found === 0 || prev === ' ' || prev === '-' || prev === '/' || prev === '.';
      score += atBoundary ? 12 : 3;
      score += streak > 0 ? 6 : 0;         // consecutive characters
      streak = idx.length && found === idx[idx.length - 1] + 1 ? streak + 1 : 0;

      idx.push(found);
      hi = found + 1;
    }

    // Shorter names win ties: "Notes" should beat "Release notes reminder".
    score -= haystack.length * 0.15;
    return { score: score, idx: idx };
  }

  function highlight(text, idx) {
    if (!idx || !idx.length) return escapeHtml(text);
    var out = '';
    var set = {};
    idx.forEach(function (i) { set[i] = true; });
    for (var i = 0; i < text.length; i++) {
      out += set[i] ? '<mark>' + escapeHtml(text[i]) + '</mark>' : escapeHtml(text[i]);
    }
    return out;
  }

  function escapeHtml(s) {
    return String(s).replace(/[&<>"']/g, function (c) {
      return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c];
    });
  }

  function build() {
    var root = document.createElement('div');
    root.className = 'palette-backdrop';
    root.hidden = true;
    root.innerHTML =
      '<div class="palette" role="dialog" aria-modal="true" aria-label="Command palette">' +
        '<div class="palette-input-row">' +
          '<svg class="icon" aria-hidden="true"><use href="#i-search"/></svg>' +
          '<input class="palette-input" type="text" role="combobox" aria-expanded="true" ' +
                 'aria-controls="palette-list" aria-autocomplete="list" autocomplete="off" ' +
                 'spellcheck="false" placeholder="Search agents, sessions, skills, settings…">' +
          '<kbd>Esc</kbd>' +
        '</div>' +
        '<div class="palette-list" id="palette-list" role="listbox" aria-label="Results"></div>' +
        '<div class="palette-footer">' +
          '<span><kbd>↑</kbd><kbd>↓</kbd> Navigate</span>' +
          '<span><kbd>Enter</kbd> Run</span>' +
          '<span><kbd>Esc</kbd> Close</span>' +
        '</div>' +
      '</div>';
    document.body.appendChild(root);
    return root;
  }

  document.addEventListener('DOMContentLoaded', function () {
    var commands = (window.Mock && window.Mock.commands) || [];
    var root = build();
    var input = root.querySelector('.palette-input');
    var list = root.querySelector('.palette-list');

    var results = [];
    var active = 0;
    var lastFocus = null;

    function render(query) {
      var scored = [];
      commands.forEach(function (c, i) {
        var m = subsequenceScore(query, c.name);
        if (!m) {
          // Fall back to the group/subtitle so "cloud" or "system" still finds
          // things whose names do not contain the word.
          var alt = subsequenceScore(query, c.group + ' ' + (c.sub || ''));
          if (!alt) return;
          scored.push({ c: c, score: alt.score - 20, idx: null, i: i });
          return;
        }
        scored.push({ c: c, score: m.score, idx: m.idx, i: i });
      });

      // Stable sort: equal scores keep the authored order, so the palette
      // opens on a predictable list rather than a shuffled one.
      scored.sort(function (a, b) { return b.score - a.score || a.i - b.i; });
      results = scored.slice(0, 60);
      active = 0;

      if (!results.length) {
        list.innerHTML = '<p class="palette-empty">Nothing matches “' + escapeHtml(query) + '”.</p>';
        input.removeAttribute('aria-activedescendant');
        return;
      }

      var html = '';
      var lastGroup = null;
      results.forEach(function (r, i) {
        // Groups only make sense while the list is still in authored order;
        // once results are ranked by score the headings would fragment.
        if (!query && r.c.group !== lastGroup) {
          lastGroup = r.c.group;
          html += '<div class="label palette-group">' + escapeHtml(lastGroup) + '</div>';
        }
        html +=
          '<button class="palette-item" role="option" id="palette-opt-' + i + '" ' +
                  'aria-selected="' + (i === active) + '" data-i="' + i + '" type="button">' +
            '<svg class="icon icon-sm" aria-hidden="true"><use href="#' + r.c.icon + '"/></svg>' +
            '<span class="palette-item-name">' + highlight(r.c.name, r.idx) + '</span>' +
            (r.c.sub ? '<span class="palette-item-sub">' + escapeHtml(r.c.sub) + '</span>' : '') +
            (r.c.keys ? '<kbd>' + escapeHtml(r.c.keys) + '</kbd>' : '') +
          '</button>';
      });
      list.innerHTML = html;
      syncActive();
    }

    function syncActive() {
      var items = list.querySelectorAll('.palette-item');
      items.forEach(function (el, i) { el.setAttribute('aria-selected', String(i === active)); });
      var el = items[active];
      if (!el) return;
      input.setAttribute('aria-activedescendant', el.id);
      // Keep the highlighted row visible without yanking the whole list about.
      el.scrollIntoView({ block: 'nearest' });
    }

    function move(delta) {
      if (!results.length) return;
      active = (active + delta + results.length) % results.length;
      syncActive();
    }

    function open() {
      lastFocus = document.activeElement;
      root.hidden = false;
      input.value = '';
      render('');
      input.focus();
    }

    function close() {
      if (root.hidden) return;
      root.hidden = true;
      // Returning focus to the trigger is the whole reason lastFocus exists:
      // without it the next Tab starts from the top of the document.
      if (lastFocus && document.contains(lastFocus)) lastFocus.focus();
    }

    function run(i) {
      var r = results[i];
      close();
      if (!r) return;
      // Nothing is wired up in a mockup; announce what would happen instead of
      // pretending an action occurred.
      if (window.Toast) {
        window.Toast.show({
          title: r.c.name,
          body: 'This is a design mockup — the command is not wired up.',
          kind: 'info'
        });
      }
    }

    input.addEventListener('input', function () { render(input.value.trim()); });

    input.addEventListener('keydown', function (ev) {
      if (ev.key === 'ArrowDown') { move(1);  ev.preventDefault(); }
      else if (ev.key === 'ArrowUp') { move(-1); ev.preventDefault(); }
      else if (ev.key === 'Home')  { active = 0; syncActive(); ev.preventDefault(); }
      else if (ev.key === 'End')   { active = results.length - 1; syncActive(); ev.preventDefault(); }
      else if (ev.key === 'Enter') { run(active); ev.preventDefault(); }
      else if (ev.key === 'Escape'){ close(); ev.preventDefault(); }
    });

    list.addEventListener('click', function (ev) {
      var el = ev.target.closest('.palette-item');
      if (el) run(parseInt(el.dataset.i, 10));
    });
    // Pointer hover follows the same selection state the keyboard uses, so the
    // two can never disagree about which row Enter would run.
    list.addEventListener('mousemove', function (ev) {
      var el = ev.target.closest('.palette-item');
      if (el) { active = parseInt(el.dataset.i, 10); syncActive(); }
    });

    root.addEventListener('mousedown', function (ev) {
      if (ev.target === root) close();
    });

    document.addEventListener('keydown', function (ev) {
      if ((ev.ctrlKey || ev.metaKey) && ev.key.toLowerCase() === 'k') {
        root.hidden ? open() : close();
        ev.preventDefault();
      }
    });

    document.querySelectorAll('[data-action="open-palette"]').forEach(function (btn) {
      btn.addEventListener('click', open);
    });

    window.Palette = { open: open, close: close };
  });
})();
