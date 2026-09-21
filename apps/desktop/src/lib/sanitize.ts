// Every piece of markdown kmdn renders comes from a collaborator: documents, PR bodies, comments.
// It is HTML by the time it reaches the DOM, so it goes through DOMPurify first (review S1).
import DOMPurify from "dompurify";
import { marked } from "marked";

const purify = DOMPurify();

// Forbid anything that can run script, load frames, or submit forms. Inline styles stay off too;
// the app's own stylesheet handles rendered markdown.
const CONFIG: Parameters<typeof purify.sanitize>[1] = {
  USE_PROFILES: { html: true },
  FORBID_TAGS: ["style", "script", "iframe", "object", "embed", "form", "input", "button", "textarea", "select", "link", "meta", "base", "svg", "math"],
  FORBID_ATTR: ["style", "srcset", "formaction", "xlink:href", "ping"],
  ALLOW_DATA_ATTR: false,
  // http(s), mailto, and relative links only. No data:, javascript:, file:, asset:.
  ALLOWED_URI_REGEXP: /^(?:(?:https?|mailto):|[^a-z]|[a-z+.-]+(?:[^a-z+.\-:]|$))/i,
};

purify.addHook("afterSanitizeAttributes", (node) => {
  if (node.tagName === "A") {
    // Never let the webview navigate. Clicks are intercepted and routed through the opener plugin.
    node.setAttribute("rel", "noopener noreferrer");
    node.removeAttribute("target");
  }
  if (node.tagName === "IMG") {
    // Relative images resolve against the document; remote images may load over https only.
    const src = node.getAttribute("src") ?? "";
    if (/^(?!https:)[a-z][a-z0-9+.-]*:/i.test(src)) node.removeAttribute("src");
    node.setAttribute("loading", "lazy");
    node.setAttribute("referrerpolicy", "no-referrer");
  }
});

export function sanitizeHtml(html: string): string {
  return purify.sanitize(html, CONFIG) as string;
}

/** GFM to safe HTML. The one renderer for Read views and diff blocks (D17). */
export function renderMarkdown(md: string): string {
  return sanitizeHtml(marked.parse(md, { async: false, gfm: true }) as string);
}

export function isExternalHref(href: string): boolean {
  return /^(https?:|mailto:)/i.test(href);
}
