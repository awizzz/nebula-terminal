/**
 * The loading screen of index.html: the eleven dots of the ❯ light up as the app gets
 * ready. Every step is a real point of the start, never a timer:
 *
 *   Starting (2 dots, drawn by index.html) → interface → shells → session → ready (11)
 *
 * The screen goes as soon as the first shell has drawn its prompt.
 */

export type SplashStep = "interface" | "shells" | "session" | "ready";

const LIT: Record<SplashStep, number> = { interface: 4, shells: 6, session: 9, ready: 11 };
const DOTS = 11;
/** Delay between two dots lighting up in one step. */
const STAGGER = 40;
/** Never keep the app behind the loading screen longer than this. */
const LONGEST = 8000;
const BOOT_KEY = "nebula-terminal.boot.v1";
/** The mark's viewBox: x and y from 240, 544 wide. */
const VIEW_ORIGIN = 240;
const VIEW_SIZE = 544;

let lit = 2;
let leaving = false;
let resolveFinished: () => void = () => undefined;

/** Resolves once the loading screen is gone. */
export const finished = new Promise<void>((resolve) => {
  resolveFinished = resolve;
});

function splash(): HTMLElement | null {
  return typeof document === "undefined" ? null : document.getElementById("splash");
}

function instant(): boolean {
  return location.hash === "#screenshot"
    || document.documentElement.dataset.animation === "off"
    || matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/** Lights the first `count` dots, the new ones one after the other, and moves the ripple to the next. */
function light(element: HTMLElement, count: number) {
  const dots = element.querySelectorAll<SVGCircleElement>("circle");
  dots.forEach((dot, index) => {
    const on = index < count;
    if (on && !dot.classList.contains("is-lit")) dot.style.setProperty("--delay", `${Math.max(0, index - lit) * STAGGER}ms`);
    dot.classList.toggle("is-lit", on);
  });
  const pulse = element.querySelector<HTMLElement>(".splash__pulse");
  const next = dots[count];
  if (pulse) {
    pulse.hidden = !next;
    if (next) {
      const percent = (value: number) => `${((value / VIEW_SIZE) * 100).toFixed(2)}%`;
      pulse.style.setProperty("--x", percent(next.cx.baseVal.value - VIEW_ORIGIN));
      pulse.style.setProperty("--y", percent(next.cy.baseVal.value - VIEW_ORIGIN));
      pulse.style.setProperty("--size", percent(next.r.baseVal.value * 2));
    }
  }
  lit = Math.max(lit, count);
  element.setAttribute("aria-valuenow", String(lit));
}

export function advance(step: SplashStep, label: string): void {
  const element = splash();
  if (!element || leaving) return;
  if (step === "ready") return finish();
  light(element, Math.max(lit, LIT[step]));
  element.setAttribute("aria-valuetext", label);
  const caption = element.querySelector(".splash__step");
  if (caption) caption.textContent = label;
}

/** Lights the last dots and fades the screen out. The app behind is usable at once. */
export function finish(): void {
  if (leaving) return;
  leaving = true;
  const element = splash();
  if (!element || instant()) {
    element?.remove();
    resolveFinished();
    return;
  }
  const remaining = DOTS - lit;
  light(element, DOTS);
  element.style.pointerEvents = "none";
  // The fade overlaps the last dots, so the screen doesn't linger once things are ready.
  window.setTimeout(() => {
    element.dataset.state = "done";
    window.setTimeout(() => {
      element.remove();
      resolveFinished();
    }, 360);
  }, Math.min(remaining, 3) * STAGGER + 60);
}

if (typeof window !== "undefined") window.setTimeout(finish, LONGEST);

/**
 * Whether a chunk of shell output draws something: the console's own set-up sequences
 * (modes, clearing the screen) come first and say nothing about the shell being ready.
 */
export function drawsText(chunk: string): boolean {
  const text = chunk
    .replace(/\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)/g, "")
    .replace(/\x1b\[[0-?]*[ -/]*[@-~]/g, "")
    .replace(/\x1b[@-_]/g, "");
  return /[^\s\x00-\x1f\x7f]/.test(text);
}

/** What the next launch's loading screen looks like, from the settings in use. */
export function rememberLook(look: { background: string; foreground: string; accent: string; scheme: "dark" | "light"; animation: string }): void {
  try {
    localStorage.setItem(BOOT_KEY, JSON.stringify(look));
  } catch {
    // Without storage the next loading screen uses the default colors.
  }
}
