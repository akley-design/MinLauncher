import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const atlasHarness = vi.hoisted(() => ({
  progress: null as unknown,
  invoke: vi.fn(),
}));

vi.mock("@minlauncher/activity-progress", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@minlauncher/activity-progress")>();
  return {
    ...actual,
    useActivityProgress: () => atlasHarness.progress,
  };
});

vi.mock("../auth", () => ({
  launcherAuth: { functions: { invoke: atlasHarness.invoke } },
}));

import type { ActivityProgressView } from "@minlauncher/activity-progress";

import { ATLAS_RELICS } from "../lib/atlas-relics";
import { ActivityAtlasLoadout } from "./ActivityAtlasLoadout";

const unlockedAtLevel11 = [
  "profile_frame_novice",
  "accent_moss",
  "badge_traveler",
  "panel_pattern_stone",
  "scene_background_island",
  "avatar_sprout",
  "accent_cave",
  "profile_frame_copper",
  "emblem_map",
  "atlas_pedestal",
  "atlas_moss",
];

function progress(equippedAtlasIds: string[] = []) {
  return {
    curveVersion: 1,
    level: 11,
    totalSeconds: 198_300,
    totalMilliseconds: 198_300_000,
    confirmedSeconds: 198_300,
    secondsIntoLevel: 0,
    requiredSeconds: 88_200,
    secondsRemaining: 88_200,
    nextLevel: 12,
    unlockedRewardIds: unlockedAtLevel11,
    equippedAtlasIds,
    creditedMilliseconds: 0,
    currentReward: {
      level: 11,
      stepSeconds: 86_400,
      totalSeconds: 198_300,
      rewardId: "atlas_moss",
      rewardLabel: "Атлас: мох",
    },
    nextReward: {
      level: 12,
      stepSeconds: 88_200,
      totalSeconds: 286_500,
      rewardId: "badge_seeker",
      rewardLabel: "Значок «Искатель»",
    },
  };
}

describe("the launcher Atlas relic loadout", () => {
  beforeEach(() => {
    atlasHarness.progress = {
      status: "active",
      progress: progress(),
      userId: "test-user",
    } satisfies ActivityProgressView;
    atlasHarness.invoke.mockReset();
  });

  afterEach(() => cleanup());

  it("keeps the full reward catalog aligned to its unlock levels", () => {
    expect(ATLAS_RELICS.map(({ id, level }) => [level, id])).toEqual([
      [10, "atlas_pedestal"],
      [11, "atlas_moss"],
      [15, "atlas_crystal"],
      [19, "atlas_lantern"],
      [23, "atlas_stones"],
      [27, "atlas_lights"],
      [30, "atlas_mini_portal"],
      [34, "atlas_constellation"],
      [38, "atlas_flowers"],
      [43, "atlas_satellite"],
      [46, "atlas_flag"],
      [49, "atlas_lighthouse"],
    ]);
  });

  it("saves an unlocked relic and leaves later rewards locked", async () => {
    atlasHarness.invoke.mockResolvedValue({
      data: progress(["atlas_pedestal"]),
      error: null,
    });
    const { rerender } = render(<ActivityAtlasLoadout />);

    fireEvent.click(screen.getByRole("button", { name: /Постамент/ }));

    await waitFor(() =>
      expect(atlasHarness.invoke).toHaveBeenCalledWith("activity-progress", {
        body: { action: "set_loadout", reward_ids: ["atlas_pedestal"] },
      }),
    );
    expect(screen.getByRole("button", { name: /Постамент/ })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByRole("button", { name: /Кристалл/ })).toBeDisabled();
    expect(screen.getByText("Откроется на уровне 15")).toBeInTheDocument();

    atlasHarness.progress = {
      status: "active",
      progress: progress(),
      userId: "another-user",
    } satisfies ActivityProgressView;
    rerender(<ActivityAtlasLoadout />);
    expect(screen.getByRole("button", { name: /Постамент/ })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
  });

  it("keeps the previous selection when the server rejects a save", async () => {
    atlasHarness.invoke.mockResolvedValue({
      data: null,
      error: new Error("offline"),
    });
    render(<ActivityAtlasLoadout />);

    fireEvent.click(screen.getByRole("button", { name: /Постамент/ }));

    expect(
      await screen.findByText("Не удалось сохранить. Проверьте соединение и попробуйте снова."),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Постамент/ })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
  });
});
