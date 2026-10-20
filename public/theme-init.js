// Applies the saved theme and accent colour before first paint to avoid a colour flash.
// Lives in its own file because the desktop CSP blocks inline scripts.
try {
  var t = localStorage.getItem("intalix.theme");
  if (t === "system") t = window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  if (t === "light" || t === "dark") document.documentElement.dataset.theme = t;
  // Custom accent colour (precomputed per theme by src/app/accent.ts).
  var vars = JSON.parse(localStorage.getItem("intalix.accent.vars") || "null");
  var themeVars = vars && vars[document.documentElement.dataset.theme === "light" ? "light" : "dark"];
  if (themeVars) for (var k in themeVars) document.documentElement.style.setProperty(k, themeVars[k]);
} catch (e) {}
