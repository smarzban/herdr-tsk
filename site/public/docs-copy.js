document.addEventListener("click", async (event) => {
  const btn = event.target.closest("[data-agent-copy]");
  if (!btn) return;
  const text = btn.getAttribute("data-copy-text") || "";
  try {
    await navigator.clipboard.writeText(text);
  } catch (_) {
    /* clipboard may be denied; the control still names the twin */
  }
});
