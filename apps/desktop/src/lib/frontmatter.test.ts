import { describe, expect, test } from "vitest";
import { readFrontmatter, relativeLink, setFrontmatterScalar } from "./frontmatter";

describe("frontmatter strip edits", () => {
  const doc = "---\n# keep\ntitle: Deploy\ntags: [ops]\nstatus: draft\n---\n# Body\n";
  test("reads fields", () => {
    expect(readFrontmatter(doc).fields).toEqual({ title: "Deploy", tags: "[ops]", status: "draft" });
    expect(readFrontmatter("# no fm").present).toBe(false);
  });
  test("replaces, adds, removes, and preserves the rest", () => {
    const a = setFrontmatterScalar(doc, "status", "published");
    expect(a).toBe("---\n# keep\ntitle: Deploy\ntags: [ops]\nstatus: published\n---\n# Body\n");
    const b = setFrontmatterScalar(a, "owner", "@alice");
    expect(b).toContain("status: published\nowner: \"@alice\"\n---");
    const c = setFrontmatterScalar(b, "owner", "");
    expect(c).not.toContain("owner:");
    expect(setFrontmatterScalar("# Body\n", "title", "New doc")).toBe("---\ntitle: New doc\n---\n# Body\n");
  });
  test("relative links", () => {
    expect(relativeLink("ops/a.md", "guides/b.md")).toBe("../guides/b.md");
    expect(relativeLink("a.md", "guides/b.md")).toBe("guides/b.md");
    expect(relativeLink("ops/deep/a.md", "ops/b.md")).toBe("../b.md");
  });
});
