/* Small, dependency-free behaviors: scheme toggle, copy buttons, output tabs,
 * scroll state, and reveal-on-scroll. Everything degrades to a working static
 * page if this file fails to load. */
(function () {
  "use strict";

  var root = document.documentElement;

  /* ---------- Color scheme ---------- */

  var STORAGE_KEY = "agc-scheme";
  var stored = null;
  try {
    stored = localStorage.getItem(STORAGE_KEY);
  } catch (e) {
    /* private mode — fall back to the OS preference */
  }
  if (stored === "light" || stored === "dark") {
    root.setAttribute("data-scheme", stored);
  }

  var toggle = document.getElementById("schemeToggle");
  if (toggle) {
    toggle.addEventListener("click", function () {
      var prefersDark =
        window.matchMedia &&
        window.matchMedia("(prefers-color-scheme: dark)").matches;
      var current = root.getAttribute("data-scheme");
      if (!current) current = prefersDark ? "dark" : "light";
      var next = current === "dark" ? "light" : "dark";
      root.setAttribute("data-scheme", next);
      try {
        localStorage.setItem(STORAGE_KEY, next);
      } catch (e) {
        /* not fatal */
      }
    });
  }

  /* ---------- Snackbar ---------- */

  var snackbar = document.getElementById("snackbar");
  var snackTimer = null;
  function toast(message) {
    if (!snackbar) return;
    snackbar.textContent = message;
    snackbar.setAttribute("data-open", "true");
    clearTimeout(snackTimer);
    snackTimer = setTimeout(function () {
      snackbar.setAttribute("data-open", "false");
    }, 2200);
  }

  /* ---------- Copy buttons ---------- */

  function copyText(text) {
    if (navigator.clipboard && navigator.clipboard.writeText) {
      return navigator.clipboard.writeText(text);
    }
    // execCommand path for browsers without the async clipboard API.
    return new Promise(function (resolve, reject) {
      var area = document.createElement("textarea");
      area.value = text;
      area.setAttribute("readonly", "");
      area.style.position = "fixed";
      area.style.opacity = "0";
      document.body.appendChild(area);
      area.select();
      try {
        document.execCommand("copy") ? resolve() : reject();
      } catch (e) {
        reject(e);
      }
      document.body.removeChild(area);
    });
  }

  Array.prototype.forEach.call(
    document.querySelectorAll("[data-copy]"),
    function (button) {
      button.addEventListener("click", function () {
        var target = document.querySelector(button.getAttribute("data-copy"));
        if (!target) return;
        copyText(target.textContent.trim()).then(
          function () {
            toast("Copied to clipboard");
          },
          function () {
            toast("Press Ctrl+C to copy");
          },
        );
      });
    },
  );

  /* ---------- Output tabs ---------- */

  var tabs = Array.prototype.slice.call(document.querySelectorAll('[role="tab"]'));
  tabs.forEach(function (tab, index) {
    tab.addEventListener("click", function () {
      tabs.forEach(function (other) {
        var panel = document.getElementById(
          other.getAttribute("aria-controls"),
        );
        var selected = other === tab;
        other.setAttribute("aria-selected", selected ? "true" : "false");
        if (panel) panel.hidden = !selected;
      });
    });
    // Left/right arrows move between tabs, per the ARIA tabs pattern.
    tab.addEventListener("keydown", function (event) {
      var delta =
        event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;
      if (!delta) return;
      event.preventDefault();
      var next = tabs[(index + delta + tabs.length) % tabs.length];
      next.focus();
      next.click();
    });
  });

  /* ---------- App bar scroll state ---------- */

  var appBar = document.getElementById("appBar");
  function syncBar() {
    if (appBar) {
      appBar.setAttribute("data-scrolled", window.scrollY > 8 ? "true" : "false");
    }
  }
  syncBar();
  window.addEventListener("scroll", syncBar, { passive: true });

  /* ---------- Reveal on scroll ---------- */

  var revealables = document.querySelectorAll(".reveal");
  if (!("IntersectionObserver" in window)) {
    Array.prototype.forEach.call(revealables, function (el) {
      el.classList.add("is-visible");
    });
    return;
  }
  var observer = new IntersectionObserver(
    function (entries) {
      entries.forEach(function (entry) {
        if (entry.isIntersecting) {
          entry.target.classList.add("is-visible");
          observer.unobserve(entry.target);
        }
      });
    },
    { rootMargin: "0px 0px -10% 0px", threshold: 0.05 },
  );
  Array.prototype.forEach.call(revealables, function (el) {
    observer.observe(el);
  });
})();
