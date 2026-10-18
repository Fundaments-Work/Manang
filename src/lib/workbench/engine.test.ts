import { describe, expect, it } from "vitest";

import {
  aboutSize,
  downloadLabel,
  explainEngineFailure,
  formatSize,
  progressFraction,
  progressLabel,
  unavailableReason,
} from "./engine";

describe("engine wording", () => {
  it("rounds sizes the way a button label wants", () => {
    expect(formatSize(14_413_816)).toBe("14.4 MB");
    expect(formatSize(250_000_000)).toBe("250 MB");
    expect(aboutSize(14_413_816)).toBe("about 14 MB");
    expect(aboutSize(100)).toBe("about 1 MB");
    expect(downloadLabel(14_413_816)).toBe(
      "Download the built-in engine (about 14 MB)",
    );
    expect(downloadLabel(undefined)).toBe("Download the built-in engine");
  });

  it("describes each step of the progress", () => {
    expect(progressLabel(null)).toBe("Starting the download…");
    expect(
      progressLabel({ phase: "downloading", received: 4_200_000, total: 14_400_000 }),
    ).toBe("Downloading 4.2 MB of 14.4 MB");
    expect(progressLabel({ phase: "verifying", received: 1, total: 1 })).toContain(
      "Checking",
    );
    expect(progressLabel({ phase: "installing", received: 1, total: 1 })).toBe(
      "Installing…",
    );
  });

  it("keeps the bar between empty and full", () => {
    expect(progressFraction(null)).toBe(0);
    expect(progressFraction({ phase: "downloading", received: 5, total: 10 })).toBe(
      0.5,
    );
    expect(progressFraction({ phase: "downloading", received: 50, total: 10 })).toBe(1);
    expect(progressFraction({ phase: "verifying", received: 10, total: 10 })).toBe(1);
    expect(progressFraction({ phase: "downloading", received: 1, total: 0 })).toBe(0);
  });

  it("explains failures in plain words and recognises a cancel", () => {
    expect(
      explainEngineFailure("[engine_cancelled] the download was cancelled"),
    ).toEqual({
      code: "engine_cancelled",
      cancelled: true,
      message: "The download was cancelled.",
    });
    const mismatch = explainEngineFailure("[engine_hash_mismatch] x");
    expect(mismatch.cancelled).toBe(false);
    expect(mismatch.message).toContain("did not match");
    expect(
      explainEngineFailure("[engine_network] could not reach h: boom").message,
    ).toContain("internet connection");
    expect(explainEngineFailure("[engine_wont_run] libicu missing").message).toContain(
      "libicu missing",
    );
    expect(explainEngineFailure("plain text").message).toBe("plain text");
  });

  it("says why an offer is unavailable", () => {
    expect(unavailableReason("engine_not_published")).toContain("not available");
    expect(unavailableReason("engine_platform_unsupported")).toContain("this computer");
    expect(unavailableReason(null)).toContain("cannot be downloaded");
  });
});
