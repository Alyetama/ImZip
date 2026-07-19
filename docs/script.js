// imzip site: scroll reveal, animated shrink bars, copy buttons

const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

// Reveal on scroll
const targets = document.querySelectorAll(".term, .shrink, .card, .mode, .ex, details");
if (reduceMotion || !("IntersectionObserver" in window)) {
  targets.forEach((el) => el.classList.add("revealed"));
} else {
  const io = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (entry.isIntersecting) {
          entry.target.classList.add("revealed");
          io.unobserve(entry.target);
        }
      }
    },
    { threshold: 0.15, rootMargin: "0px 0px -40px 0px" }
  );
  targets.forEach((el) => io.observe(el));
}

// Copy buttons
// .install-line already carries an explicit <button>; terminal/example blocks
// get one injected in the top-right corner.
function copyText(text, button) {
  navigator.clipboard.writeText(text).then(() => {
    button.textContent = "Copied";
    button.classList.add("done");
    setTimeout(() => {
      button.textContent = "Copy";
      button.classList.remove("done");
    }, 1600);
  });
}

document.querySelectorAll("[data-copy]").forEach((block) => {
  const existing = block.querySelector(".copy");
  const code = block.querySelector("pre code, code");
  if (!code) return;
  // Strip the shell prompt and comment-only lines from what gets copied.
  const raw = code.innerText
    .split("\n")
    .map((l) => l.replace(/^\$\s?/, ""))
    .filter((l) => !l.trim().startsWith("#") && l.trim() !== "")
    .join("\n");

  if (existing) {
    existing.addEventListener("click", () => copyText(raw, existing));
    return;
  }
  // Floating buttons only on example and config blocks; the hero terminal
  // is illustrative and keeps its clean title bar.
  if (!block.matches(".ex, .term.small")) return;
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "copy floating";
  btn.textContent = "Copy";
  btn.addEventListener("click", () => copyText(raw, btn));
  block.style.position = "relative";
  block.appendChild(btn);
});

// Theme toggle. Dark is the default; the choice is persisted.
const themeToggle = document.querySelector(".theme-toggle");
if (themeToggle) {
  themeToggle.addEventListener("click", () => {
    const next = document.documentElement.dataset.theme === "dark" ? "light" : "dark";
    document.documentElement.dataset.theme = next;
    themeToggle.setAttribute("aria-pressed", String(next === "dark"));
    try {
      localStorage.setItem("imzip-theme", next);
    } catch (e) {}
  });
}
