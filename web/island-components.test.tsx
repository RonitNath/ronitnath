import { render } from "@solidjs/testing-library";
import { describe, expect, it } from "vitest";

import PublicRoot from "./islands/PublicRoot";

describe("generated island components", () => {
  it("public root owns one main landmark", () => {
    const result = render(() => <PublicRoot />);
    expect(result.container.querySelectorAll("main")).toHaveLength(1);
  });
});
