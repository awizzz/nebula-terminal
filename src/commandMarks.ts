/**
 * The line to scroll to when jumping between commands, given the first line of
 * each prompt and the line at the top of the screen. Going up reveals the nearest
 * prompt above the screen; going down, the next one below the top line.
 */
export function adjacentPrompt(prompts: readonly number[], top: number, direction: -1 | 1): number | undefined {
  let found: number | undefined;
  for (const line of prompts) {
    if (direction < 0 ? line < top && (found === undefined || line > found) : line > top && (found === undefined || line < found)) {
      found = line;
    }
  }
  return found;
}

/** Buffer lines back to text: a wrapped line continues the previous one instead of starting a new one. */
export function joinLines(lines: ReadonlyArray<{ text: string; wrapped: boolean }>): string {
  let text = "";
  lines.forEach((line, index) => {
    text += (index > 0 && !line.wrapped ? "\n" : "") + line.text;
  });
  return text.replace(/\s+$/, "");
}

/**
 * The command typed on a prompt line, without the prompt: Nebula's `❯`, PowerShell's
 * `PS C:\…>`, cmd's `C:\…>` and bash's `user@host:~$`. Empty when the line holds
 * nothing but a prompt.
 */
export function commandOnPromptLine(line: string): string {
  const patterns = [
    /^\s*[❯›»λ➜→]\s?/u,
    /^\s*PS [^>]*>\s?/,
    /^\s*[A-Za-z]:\\[^>]*>\s?/,
    /^[^\s$#]*[@:][^\s$#]*[$#]\s?/,
    /^\s*[$#%>]\s/,
  ];
  for (const pattern of patterns) {
    const match = pattern.exec(line);
    if (match) return line.slice(match[0].length).trim();
  }
  return line.trim();
}
