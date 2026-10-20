// Apply the saved (or OS-preferred) theme before first paint so dark mode
// never flashes white. Mirrors initialTheme() and THEME_COLOR in App.tsx.
// A file rather than an inline script, so the server's Content-Security-Policy
// can keep script-src to 'self'; index.html loads it as a blocking script.
(function () {
  try {
    var t = localStorage.getItem("theme");
    if (t !== "light" && t !== "dark")
      t = matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
    document.documentElement.dataset.theme = t;
    if (t === "dark")
      document
        .querySelector('meta[name="theme-color"]')
        .setAttribute("content", "#16181c");
  } catch (e) {}
})();
