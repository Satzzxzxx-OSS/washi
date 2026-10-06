const RESET_MS = 1200;

export function addCopyButtons(root: ParentNode) {
  for (const pre of root.querySelectorAll<HTMLElement>("pre")) {
    const code = pre.querySelector("code");
    if (!code || pre.querySelector(".copy")) continue;

    const button = document.createElement("button");
    button.type = "button";
    button.className = "copy";
    button.textContent = "コピー";
    button.addEventListener("click", async () => {
      try {
        await navigator.clipboard.writeText(code.textContent ?? "");
        button.textContent = "コピーしました";
      } catch {
        button.textContent = "失敗しました";
      }
      setTimeout(() => (button.textContent = "コピー"), RESET_MS);
    });
    pre.append(button);
  }
}
