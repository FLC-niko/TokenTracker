import { describe, expect, it } from "vitest";
import { scrubEvent } from "../sentry.js";

describe("dashboard sentry scrubEvent", () => {
  it("redacts secrets and strips user PII fields", () => {
    const scrubbed = scrubEvent({
      message: "Bearer abcdefghijklmnop",
      extra: { refresh_token: "xyz", safe: 1 },
      user: { id: "u1", email: "x@y.z" },
    });
    expect(scrubbed.message).toMatch(/Bearer \[Filtered\]/);
    expect(scrubbed.extra.refresh_token).toBe("[Filtered]");
    expect(scrubbed.extra.safe).toBe(1);
    expect(scrubbed.user).toEqual({ id: "u1" });
  });
});
