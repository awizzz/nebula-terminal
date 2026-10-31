// Runs before the window is first drawn, so the loading screen already wears the
// colors of the last session. The app writes this record (src/splash.ts).
(function () {
  var root = document.documentElement;
  try {
    var boot = JSON.parse(localStorage.getItem("nebula-terminal.boot.v1") || "null");
    if (boot && typeof boot === "object") {
      var color = function (value) {
        return typeof value === "string" && /^#[0-9a-f]{6}$/i.test(value) ? value : null;
      };
      [["--term-bg", boot.background], ["--term-fg", boot.foreground], ["--accent", boot.accent]].forEach(function (entry) {
        var value = color(entry[1]);
        if (value) root.style.setProperty(entry[0], value);
      });
      if (boot.scheme === "light") root.dataset.scheme = "light";
      if (boot.animation === "reduced" || boot.animation === "off") root.dataset.animation = boot.animation;
    }
  } catch (error) {
    // No storage, or a record from elsewhere: the default colors will do.
  }
  // Documentation screenshots skip the loading screen.
  if (location.hash === "#screenshot") root.dataset.animation = "off";
})();
