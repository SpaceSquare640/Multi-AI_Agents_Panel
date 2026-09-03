/* ============================================================================
   Shell
   ----------------------------------------------------------------------------
   Panel collapse/pin, panel resizing, and rail navigation for the mockup.
   Nothing here is application logic — it exists so the layout can actually be
   exercised in a browser rather than judged from a still image.
   ========================================================================== */

(function () {
  var KEY = 'maap.shell';

  function loadState() {
    try { return JSON.parse(localStorage.getItem(KEY) || '{}'); }
    catch (e) { return {}; }
  }
  function saveState(s) {
    try { localStorage.setItem(KEY, JSON.stringify(s)); } catch (e) { /* not fatal */ }
  }

  document.addEventListener('DOMContentLoaded', function () {
    var shell = document.querySelector('.shell');
    if (!shell) return;

    var state = loadState();
    if (state.sidebar)   shell.dataset.sidebar = state.sidebar;
    if (state.inspector) shell.dataset.inspector = state.inspector;
    if (state.sidebarW)   shell.style.setProperty('--sidebar-w', state.sidebarW);
    if (state.inspectorW) shell.style.setProperty('--inspector-w', state.inspectorW);

    /* ---- collapse toggles ------------------------------------------------
       aria-expanded is kept in sync so the control announces its own state
       rather than relying on an icon rotation nobody can hear. */
    function toggle(panel, btn) {
      var collapsed = shell.dataset[panel] === 'collapsed';
      shell.dataset[panel] = collapsed ? 'expanded' : 'collapsed';
      state[panel] = shell.dataset[panel];
      saveState(state);
      if (btn) btn.setAttribute('aria-expanded', String(collapsed));
    }

    document.querySelectorAll('[data-action="toggle-sidebar"]').forEach(function (btn) {
      btn.setAttribute('aria-expanded', String(shell.dataset.sidebar !== 'collapsed'));
      btn.addEventListener('click', function () { toggle('sidebar', btn); });
    });
    document.querySelectorAll('[data-action="toggle-inspector"]').forEach(function (btn) {
      btn.setAttribute('aria-expanded', String(shell.dataset.inspector !== 'collapsed'));
      btn.addEventListener('click', function () { toggle('inspector', btn); });
    });

    /* ---- resizers --------------------------------------------------------
       Pointer events rather than mouse events so a pen or touch drag behaves
       the same. Width is clamped to the min/max component tokens so a panel
       can never be dragged to an unusable size or off screen. */
    document.querySelectorAll('.resizer').forEach(function (el) {
      var target = el.dataset.resize;                 // "sidebar" | "inspector"
      var dir = target === 'inspector' ? -1 : 1;      // inspector grows leftward
      var varName = '--' + target + '-w';
      var minVar = '--' + target + '-w-min';
      var maxVar = '--' + target + '-w-max';

      el.setAttribute('role', 'separator');
      el.setAttribute('aria-orientation', 'vertical');
      el.setAttribute('tabindex', '0');
      el.setAttribute('aria-label', 'Resize ' + target + ' panel');

      function px(v) { return parseInt(v, 10) || 0; }
      function cs(name) {
        return px(getComputedStyle(shell).getPropertyValue(name));
      }
      function setWidth(w) {
        var min = cs(minVar), max = cs(maxVar);
        w = Math.max(min, Math.min(max, w));
        shell.style.setProperty(varName, w + 'px');
        state[target + 'W'] = w + 'px';
        saveState(state);
      }

      el.addEventListener('pointerdown', function (ev) {
        // Dragging a collapsed panel would silently do nothing; expand first.
        if (shell.dataset[target] === 'collapsed') {
          shell.dataset[target] = 'expanded';
          state[target] = 'expanded';
        }
        el.dataset.dragging = 'true';
        el.setPointerCapture(ev.pointerId);
        var startX = ev.clientX;
        var startW = cs(varName);
        // The grid transition must be off during a drag, or the panel lags
        // behind the pointer by a full 200ms.
        shell.style.transition = 'none';

        function move(e) { setWidth(startW + (e.clientX - startX) * dir); }
        function up(e) {
          el.releasePointerCapture(ev.pointerId);
          delete el.dataset.dragging;
          shell.style.transition = '';
          el.removeEventListener('pointermove', move);
          el.removeEventListener('pointerup', up);
        }
        el.addEventListener('pointermove', move);
        el.addEventListener('pointerup', up);
      });

      // Keyboard equivalent, required because drag-only resizing is
      // unreachable without a pointer.
      el.addEventListener('keydown', function (ev) {
        var step = ev.shiftKey ? 32 : 8;
        if (ev.key === 'ArrowLeft')  { setWidth(cs(varName) - step * dir); ev.preventDefault(); }
        if (ev.key === 'ArrowRight') { setWidth(cs(varName) + step * dir); ev.preventDefault(); }
        if (ev.key === 'Home')       { setWidth(cs(minVar)); ev.preventDefault(); }
        if (ev.key === 'End')        { setWidth(cs(maxVar)); ev.preventDefault(); }
      });
    });

    /* ---- rail tooltips ---------------------------------------------------
       The rail is nine icon-only destinations. Their accessible names are
       already in .sr-only, but a sighted user hovering gets whatever delay the
       browser gives native `title` — around a second, and unstyled. The
       .tooltip component and --tooltip-delay existed for this and had no
       consumer, so the rail relied on the thing they were written to replace.

       title is removed once JS takes over, otherwise both appear. Focus shows
       the tooltip immediately: a keyboard user arriving on an item has already
       committed to it, and making them wait is pointless. */
    (function railTooltips() {
      var rail = document.querySelector('.rail');
      if (!rail) return;

      var delay = parseFloat(getComputedStyle(document.documentElement)
                    .getPropertyValue('--tooltip-delay')) || 400;
      var tip = null, timer = null;

      function hide() {
        clearTimeout(timer);
        if (tip) { tip.remove(); tip = null; }
      }

      function show(item, immediate) {
        hide();
        var label = item.dataset.tip;
        if (!label) return;
        function place() {
          tip = document.createElement('div');
          tip.className = 'tooltip';
          tip.setAttribute('role', 'presentation');   // the name is on the item
          tip.textContent = label;
          document.body.appendChild(tip);
          var r = item.getBoundingClientRect();
          tip.style.left = (r.right + 8) + 'px';
          tip.style.top = Math.round(r.top + (r.height - tip.offsetHeight) / 2) + 'px';
        }
        if (immediate) place();
        else timer = setTimeout(place, delay);
      }

      rail.querySelectorAll('.rail-item').forEach(function (item) {
        var name = item.querySelector('.sr-only');
        var group = item.closest('[role="group"]');
        item.dataset.tip = (name ? name.textContent : '') +
          (group && group.getAttribute('aria-label') ? ' · ' + group.getAttribute('aria-label') : '');
        item.removeAttribute('title');

        item.addEventListener('mouseenter', function () { show(item, false); });
        item.addEventListener('mouseleave', hide);
        item.addEventListener('focus', function () { show(item, true); });
        item.addEventListener('blur', hide);
        item.addEventListener('click', hide);
      });

      window.addEventListener('scroll', hide, true);
      document.addEventListener('keydown', function (ev) { if (ev.key === 'Escape') hide(); });
    })();

    /* ---- rail navigation -------------------------------------------------
       The rail is a real toolbar: arrow keys move between destinations and
       only the active item is in the tab order, which is the expected
       behaviour for a toolbar and keeps Tab moving between regions rather
       than through nine buttons. */
    var rail = document.querySelector('.rail');
    if (rail) {
      var items = Array.prototype.slice.call(rail.querySelectorAll('.rail-item'));

      function focusItem(i) {
        var n = items.length;
        var next = items[((i % n) + n) % n];
        items.forEach(function (it) { it.tabIndex = -1; });
        next.tabIndex = 0;
        next.focus();
      }

      items.forEach(function (item, i) {
        item.tabIndex = item.getAttribute('aria-current') === 'page' ? 0 : -1;
        item.addEventListener('keydown', function (ev) {
          if (ev.key === 'ArrowDown' || ev.key === 'ArrowRight') { focusItem(i + 1); ev.preventDefault(); }
          if (ev.key === 'ArrowUp'   || ev.key === 'ArrowLeft')  { focusItem(i - 1); ev.preventDefault(); }
          if (ev.key === 'Home') { focusItem(0); ev.preventDefault(); }
          if (ev.key === 'End')  { focusItem(items.length - 1); ev.preventDefault(); }
        });
      });
    }
  });
})();
