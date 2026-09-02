(() => {
  const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  // ── reveal on scroll ─────────────────────────────────────────────────────
  const reveal = document.querySelectorAll("[data-reveal]");
  if ("IntersectionObserver" in window && !reduced) {
    const io = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            entry.target.classList.add("is-in");
            io.unobserve(entry.target);
          }
        }
      },
      { threshold: 0.12, rootMargin: "0px 0px -6% 0px" },
    );
    reveal.forEach((el) => io.observe(el));
  } else {
    reveal.forEach((el) => el.classList.add("is-in"));
  }

  // ── status cycle: one row walking through the five human states ──────────
  const cycle = document.querySelector("[data-cycle]");
  if (cycle) {
    const steps = [
      { sec: "ON DECK", glyph: "○", status: "ready", key: "ctrl+s", next: "started" },
      { sec: "IN MOTION", glyph: "▸", status: "started", key: "ctrl+b", next: "blocked" },
      { sec: "ON DECK", glyph: "■", status: "blocked", key: "ctrl+b", next: "ready" },
      { sec: "ON DECK", glyph: "○", status: "ready", key: ": review", next: "review" },
      { sec: "ON DECK", glyph: "▲", status: "review", key: "ctrl+d", next: "done" },
      { sec: "DONE", glyph: "✓", status: "done", key: "ctrl+o", next: "ready" },
    ];
    const sec = cycle.querySelector("[data-cycle-sec]");
    const glyph = cycle.querySelector("[data-cycle-glyph]");
    const key = cycle.querySelector("[data-cycle-key]");
    const next = cycle.querySelector("[data-cycle-next]");
    let i = 0;
    const paint = () => {
      const s = steps[i];
      sec.textContent = s.sec;
      glyph.textContent = s.glyph;
      key.textContent = s.key;
      next.textContent = s.next;
      cycle.classList.remove("is-tick");
      // restart the tick animation
      void cycle.offsetWidth;
      cycle.classList.add("is-tick");
    };
    paint();
    if (!reduced) {
      let timer = null;
      const start = () => {
        if (timer) return;
        timer = window.setInterval(() => {
          i = (i + 1) % steps.length;
          paint();
        }, 2200);
      };
      const stop = () => {
        if (!timer) return;
        window.clearInterval(timer);
        timer = null;
      };
      if ("IntersectionObserver" in window) {
        const io = new IntersectionObserver(
          (entries) => {
            entries.forEach((e) => (e.isIntersecting ? start() : stop()));
          },
          { threshold: 0.2 },
        );
        io.observe(cycle);
      } else {
        start();
      }
      document.addEventListener("visibilitychange", () => {
        if (document.hidden) stop();
      });
    }
  }

  // ── install tabs ─────────────────────────────────────────────────────────
  const install = document.querySelector("[data-install]");
  if (install) {
    const tabs = [...install.querySelectorAll("[data-install-tab]")];
    const panels = [...install.querySelectorAll("[data-install-panel]")];
    const select = (name, focus = false) => {
      tabs.forEach((tab) => {
        const on = tab.dataset.installTab === name;
        tab.setAttribute("aria-selected", on ? "true" : "false");
        tab.tabIndex = on ? 0 : -1;
        if (on && focus) tab.focus();
      });
      panels.forEach((panel) => {
        panel.hidden = panel.dataset.installPanel !== name;
      });
    };
    tabs.forEach((tab, idx) => {
      tab.addEventListener("click", () => select(tab.dataset.installTab));
      tab.addEventListener("keydown", (e) => {
        if (e.key !== "ArrowRight" && e.key !== "ArrowLeft") return;
        e.preventDefault();
        const delta = e.key === "ArrowRight" ? 1 : -1;
        const nextTab = tabs[(idx + delta + tabs.length) % tabs.length];
        select(nextTab.dataset.installTab, true);
      });
    });
  }

  // ── copy buttons ─────────────────────────────────────────────────────────
  document.querySelectorAll("[data-copy]").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const wrap = btn.closest(".shell-wrap");
      const pre = wrap && wrap.querySelector("[data-copy-source]");
      if (!pre) return;
      // Strip the "$ " prompts so the paste runs as-is.
      const text = pre.innerText
        .split("\n")
        .map((line) => line.replace(/^\$\s?/, ""))
        .filter((line) => line.trim().length)
        .join("\n");
      let ok = false;
      try {
        await navigator.clipboard.writeText(text);
        ok = true;
      } catch (_) {
        ok = false;
      }
      const label = btn.textContent;
      btn.textContent = ok ? "copied" : "select to copy";
      btn.classList.toggle("is-done", ok);
      window.setTimeout(() => {
        btn.textContent = label;
        btn.classList.remove("is-done");
      }, 1400);
    });
  });
})();
