/* ============================================================================
   Shared UI behaviour: toasts and dialogs
   ----------------------------------------------------------------------------
   Small enough to live together, and both are needed by more than one screen.
   ========================================================================== */

(function () {

  /* ==========================================================================
     Toast
     A polite live region: announcements reach a screen reader without stealing
     focus, because interrupting whatever the user is doing to report a
     background event is worse than the event going unheard for a second.
     ========================================================================== */

  var region = null;

  function ensureRegion() {
    if (region) return region;
    region = document.createElement('div');
    region.className = 'toast-region';
    region.setAttribute('role', 'status');
    region.setAttribute('aria-live', 'polite');
    document.body.appendChild(region);
    return region;
  }

  function dismiss(el) {
    if (!el || el.dataset.closing) return;
    el.dataset.closing = 'true';
    // Wait for the exit animation, but never hang on it: with reduced motion
    // the durations collapse to 1ms, and a missed animationend would otherwise
    // leave the node in the DOM forever.
    var done = false;
    function remove() { if (!done) { done = true; el.remove(); } }
    el.addEventListener('animationend', remove, { once: true });
    setTimeout(remove, 400);
  }

  var Toast = {
    show: function (opts) {
      var host = ensureRegion();
      var el = document.createElement('div');
      el.className = 'toast';
      el.dataset.kind = opts.kind || 'info';

      var html = '<div class="toast-body">';
      if (opts.title) html += '<strong>' + esc(opts.title) + '</strong>';
      if (opts.body) html += esc(opts.body);
      html += '</div>';
      if (opts.action) html += '<button class="toast-action" type="button">' + esc(opts.action) + '</button>';
      html += '<button class="btn-icon btn-sm" type="button" data-close aria-label="Dismiss notification">' +
                '<svg class="icon icon-sm" aria-hidden="true"><use href="#i-close"/></svg></button>';
      el.innerHTML = html;

      el.querySelector('[data-close]').addEventListener('click', function () { dismiss(el); });
      var action = el.querySelector('.toast-action');
      if (action) action.addEventListener('click', function () {
        if (opts.onAction) opts.onAction();
        dismiss(el);
      });

      host.appendChild(el);

      // An undoable action gets longer than the default: five seconds is not
      // enough to read a message and decide to take it back.
      var life = opts.timeout || (opts.action ? 9000 : 5000);
      var timer = setTimeout(function () { dismiss(el); }, life);
      // Hovering pauses the countdown, so a toast cannot expire while it is
      // being read.
      el.addEventListener('mouseenter', function () { clearTimeout(timer); });
      el.addEventListener('mouseleave', function () { timer = setTimeout(function () { dismiss(el); }, 2500); });

      return el;
    }
  };

  function esc(s) {
    return String(s).replace(/[&<>"']/g, function (c) {
      return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c];
    });
  }

  /* ==========================================================================
     Dialog
     Thin wrapper over the native <dialog>. The platform already provides focus
     trapping, Esc, inertness of the page behind and the backdrop; re-writing
     any of that by hand would only be a worse version of it. This adds the
     exit animation and the confirm/cancel plumbing.
     ========================================================================== */

  function closeWithAnimation(dlg, value) {
    if (dlg.dataset.closing) return;
    dlg.dataset.closing = 'true';
    var done = false;
    function finish() {
      if (done) return;
      done = true;
      delete dlg.dataset.closing;
      dlg.close(value || '');
    }
    dlg.addEventListener('animationend', finish, { once: true });
    setTimeout(finish, 300);
  }

  var Dialog = {
    open: function (dlg) {
      if (typeof dlg === 'string') dlg = document.getElementById(dlg);
      if (!dlg || dlg.open) return;
      dlg.showModal();
      // Focus the least destructive control rather than whatever happens to be
      // first, so a stray Enter cannot confirm something irreversible.
      var safe = dlg.querySelector('[data-autofocus]') ||
                 dlg.querySelector('.btn-secondary') ||
                 dlg.querySelector('button');
      if (safe) safe.focus();
    },
    close: closeWithAnimation
  };

  document.addEventListener('DOMContentLoaded', function () {
    document.querySelectorAll('[data-dialog-open]').forEach(function (btn) {
      btn.addEventListener('click', function () { Dialog.open(btn.dataset.dialogOpen); });
    });
    document.querySelectorAll('dialog').forEach(function (dlg) {
      dlg.querySelectorAll('[data-dialog-close]').forEach(function (btn) {
        btn.addEventListener('click', function () {
          closeWithAnimation(dlg, btn.dataset.dialogClose);
        });
      });
      // Esc fires the native cancel event, which would skip the exit
      // animation; intercept it and take the same path a button does.
      dlg.addEventListener('cancel', function (ev) {
        ev.preventDefault();
        closeWithAnimation(dlg, 'cancel');
      });
      dlg.addEventListener('close', function () {
        if (!dlg.returnValue || dlg.returnValue === 'cancel') return;
        var msg = dlg.dataset.confirmToast;
        if (msg) Toast.show({ title: msg, body: 'This is a design mockup — nothing was changed.', kind: 'success' });
      });
    });
  });

  window.Toast = Toast;
  window.Dialog = Dialog;
})();
