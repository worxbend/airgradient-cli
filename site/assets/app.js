/* Progressive enhancement: the screenshots, commands and FAQs need no JS. */
(() => {
  "use strict";
  const toast = document.getElementById("toast");
  let toastTimer;

  function announce(message) {
    clearTimeout(toastTimer);
    toast.textContent = message;
    toastTimer = setTimeout(() => { toast.textContent = ""; }, 5000);
  }

  function selectCommand(target) {
    const range = document.createRange();
    range.selectNodeContents(target);
    const selection = window.getSelection();
    selection.removeAllRanges();
    selection.addRange(range);
  }

  document.querySelectorAll("[data-copy]").forEach((button) => {
    const target = document.getElementById(button.dataset.copy);
    if (!target) return;
    button.hidden = false;
    button.addEventListener("click", async () => {
      try {
        if (!navigator.clipboard) throw new Error("Clipboard unavailable");
        await navigator.clipboard.writeText(target.textContent.trim());
        announce("Command copied. Ready for your terminal.");
      } catch {
        selectCommand(target);
        announce("Copy unavailable. Command selected: press Ctrl+C or ⌘C, or long-press to copy.");
      }
    });
  });

  const tabs = Array.from(document.querySelectorAll('[role="tab"]'));
  function activateTab(selected) {
    tabs.forEach((tab) => {
      const active = tab === selected;
      tab.setAttribute("aria-selected", String(active));
      tab.tabIndex = active ? 0 : -1;
      document.getElementById(tab.getAttribute("aria-controls")).hidden = !active;
    });
  }
  tabs.forEach((tab, index) => {
    tab.addEventListener("click", () => activateTab(tab));
    tab.addEventListener("keydown", (event) => {
      const destinations = {
        ArrowRight: (index + 1) % tabs.length,
        ArrowLeft: (index + tabs.length - 1) % tabs.length,
        Home: 0,
        End: tabs.length - 1,
      };
      if (!(event.key in destinations)) return;
      event.preventDefault();
      const next = tabs[destinations[event.key]];
      activateTab(next);
      next.focus();
    });
  });
  activateTab(tabs[0]);
  document.getElementById("view-tabs").hidden = false;

  const themes = {
    default: { file: "tui-dashboard.png", label: "Default" },
    nord: { file: "tui-nord.png", label: "Nord" },
    catppuccin: { file: "tui-catppuccin.png", label: "Catppuccin Mocha" },
  };
  const dashboard = document.getElementById("dashboard-image");
  const fullSize = document.getElementById("dashboard-full");
  const themeButtons = document.querySelectorAll("[data-theme]");
  let requestedTheme = "default";

  function applyTheme(id, preview) {
    // A late image load must not overwrite a more recent selection.
    if (requestedTheme !== id) return;
    const theme = themes[id];
    dashboard.src = preview.src;
    fullSize.href = preview.src;
    dashboard.alt = `${theme.label} theme: air-quality dashboard with AQI, metric cards, banded gauges and trends. Synthetic demo readings.`;
    themeButtons.forEach((button) => {
      button.setAttribute("aria-pressed", String(button.dataset.theme === id));
    });
    announce(`${theme.label} dashboard preview.`);
  }
  themeButtons.forEach((button) => {
    button.addEventListener("click", () => {
      const id = button.dataset.theme;
      requestedTheme = id;
      const preview = new Image();
      preview.onload = () => applyTheme(id, preview);
      preview.onerror = () => {
        if (requestedTheme === id) announce("Preview could not load. Showing the previous theme.");
      };
      preview.src = `assets/screenshots/${themes[id].file}`;
    });
  });
  document.getElementById("palette-bar").hidden = false;
})();
