/* ============================================================================
   Shell frame
   ----------------------------------------------------------------------------
   The titlebar, rail, panel resizers and status bar are identical on every
   screen, so they live here rather than being copied into each page. Twelve
   hand-maintained copies of the same chrome is how twelve screens end up
   disagreeing about what the rail contains.

   A page supplies only what is actually its own:

       <div class="shell" data-screen="chat">
         <aside class="sidebar">…</aside>
         <main class="workspace" id="workspace">…</main>
         <aside class="inspector">…</aside>
       </div>

   and this fills in the rest around it, marking the rail item whose id
   matches data-screen as current.
   ========================================================================== */

(function () {

  var RAIL = [
    { group: 'Workspace', items: [
      { id: 'chat',   label: 'Chat',            icon: 'i-chat',   href: 'chat.html' },
      { id: 'notes',  label: 'Notes',           icon: 'i-notes',  href: 'notes.html' }
    ]},
    { group: 'Capabilities', items: [
      { id: 'models', label: 'Models',          icon: 'i-models', href: 'models.html' },
      { id: 'skills', label: 'Skills',          icon: 'i-skills', href: 'skills.html', dot: true },
      { id: 'search', label: 'Semantic Search', icon: 'i-search', href: 'semantic-search.html' },
      { id: 'game',   label: 'Game Agent',      icon: 'i-game',   href: 'game-agent.html' }
    ]},
    { group: 'System', items: [
      { id: 'usage',    label: 'Usage',    icon: 'i-usage',    href: 'usage.html' },
      { id: 'settings', label: 'Settings', icon: 'i-settings', href: 'settings.html' },
      { id: 'help',     label: 'Help',     icon: 'i-help',     href: 'help.html' }
    ]}
  ];

  function icon(id, cls) {
    return '<svg class="icon ' + (cls || '') + '" aria-hidden="true"><use href="#' + id + '"/></svg>';
  }

  function titlebar() {
    return '' +
      '<header class="titlebar">' +
        '<span class="titlebar-brand"><img src="../app-icon.png" alt="">Multi-AI Agents Panel</span>' +
        '<button class="titlebar-search" data-action="open-palette" type="button">' +
          icon('i-search', 'icon-sm') +
          '<span>Search agents, sessions, skills, settings…</span>' +
          '<kbd>Ctrl K</kbd>' +
        '</button>' +
        '<div class="titlebar-actions">' +
          '<button class="titlebar-btn" data-action="toggle-sidebar" type="button" aria-label="Toggle sidebar">' + icon('i-panel-left', 'icon-sm') + '</button>' +
          '<button class="titlebar-btn" data-action="toggle-inspector" type="button" aria-label="Toggle inspector">' + icon('i-panel-right', 'icon-sm') + '</button>' +
          '<button class="titlebar-btn" data-action="theme-cycle" type="button" aria-label="Change theme">' + icon('i-theme', 'icon-sm') + '</button>' +
        '</div>' +
      '</header>';
  }

  function rail(current) {
    var html = '<nav class="rail" aria-label="Primary">';
    RAIL.forEach(function (g, gi) {
      // The system group is pushed to the bottom: configuration is not
      // where the work happens, and separating it spatially says so.
      if (gi === RAIL.length - 1) html += '<span class="rail-spacer"></span>';
      html += '<div class="rail-group" role="group" aria-label="' + g.group + '">';
      g.items.forEach(function (it) {
        var isCurrent = it.id === current;
        html +=
          '<a class="rail-item" href="' + it.href + '" title="' + it.label + ' — ' + g.group + '"' +
             (isCurrent ? ' aria-current="page"' : '') + '>' +
            icon(it.icon) +
            '<span class="sr-only">' + it.label + '</span>' +
            (it.dot ? '<span class="dot" aria-hidden="true"></span>' : '') +
          '</a>';
      });
      html += '</div>';
    });
    return html + '</nav>';
  }

  function statusbar() {
    var u = (window.Mock && window.Mock.usage) || { todayCost: 0 };
    return '' +
      '<footer class="statusbar">' +
        '<button class="statusbar-item" type="button">' +
          '<span class="status-dot" data-state="ok" aria-hidden="true"></span>' +
          icon('i-shield', 'icon-sm') + 'Guardrails enforced' +
        '</button>' +
        '<span class="statusbar-item">' +
          '<span class="status-dot" data-state="running" aria-hidden="true"></span>2 agents running' +
        '</span>' +
        '<span class="statusbar-spacer"></span>' +
        '<span class="statusbar-item">Ollama <span class="status-dot" data-state="ok" aria-hidden="true"></span></span>' +
        '<span class="statusbar-item">Today <span class="num">$' + u.todayCost.toFixed(2) + '</span></span>' +
        '<span class="statusbar-item">EN</span>' +
      '</footer>';
  }

  document.addEventListener('DOMContentLoaded', function () {
    var shell = document.querySelector('.shell');
    if (!shell || shell.dataset.framed) return;

    var sidebar   = shell.querySelector('.sidebar');
    var workspace = shell.querySelector('.workspace');
    var inspector = shell.querySelector('.inspector');

    // Rebuilt in grid order so the resizers land in the right seams. The
    // page's own regions are moved, not re-created, so any listener a screen
    // attached before this ran survives.
    var frag = document.createDocumentFragment();
    var wrap = document.createElement('div');

    wrap.innerHTML = titlebar() + rail(shell.dataset.screen);
    while (wrap.firstChild) frag.appendChild(wrap.firstChild);

    if (sidebar) {
      frag.appendChild(sidebar);
      var r1 = document.createElement('span');
      r1.className = 'resizer';
      r1.dataset.resize = 'sidebar';
      frag.appendChild(r1);
    }
    if (workspace) frag.appendChild(workspace);
    if (inspector) {
      var r2 = document.createElement('span');
      r2.className = 'resizer';
      r2.dataset.resize = 'inspector';
      frag.appendChild(r2);
      frag.appendChild(inspector);
    }

    wrap.innerHTML = statusbar();
    while (wrap.firstChild) frag.appendChild(wrap.firstChild);

    shell.textContent = '';
    shell.appendChild(frag);
    shell.dataset.framed = 'true';

    // A screen with no inspector should not leave a 312px gap where one would
    // have been.
    if (!inspector) shell.dataset.inspector = 'collapsed';
    if (!sidebar)   shell.dataset.sidebar = 'collapsed';

    document.dispatchEvent(new CustomEvent('framed'));
  });
})();
