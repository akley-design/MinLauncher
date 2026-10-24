/**
 * The screen of the performance.
 *
 * The screen shows what was measured, and nothing else. The two things
 * it has to get right are the table of the stored measurements and the
 * pair of the game — the run with the mod and the run without it, which
 * only exists when both of them were measured.
 */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { BenchmarkList, InGameComparisons } from "../ipc/types";
import { useGameStore } from "../stores/game";
import { PerformanceRoute } from "./performance";

// The backend is replaced by a mock, so that the test does not need a
// running desktop application.
const latestBenchmarks = vi.fn<() => Promise<BenchmarkList>>();
const compareInGame = vi.fn<() => Promise<InGameComparisons>>();

vi.mock("../ipc/client", async () => {
  const actual = await vi.importActual<typeof import("../ipc/client")>("../ipc/client");
  return {
    ...actual,
    latestBenchmarks: () => latestBenchmarks(),
    compareInGame: () => compareInGame(),
  };
});

/** The screen, with a fresh query cache around it. */
function renderScreen() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });

  return render(
    <QueryClientProvider client={client}>
      <PerformanceRoute />
    </QueryClientProvider>,
  );
}

describe("the screen of the performance", () => {
  beforeEach(() => {
    latestBenchmarks.mockReset();
    compareInGame.mockReset();
    latestBenchmarks.mockResolvedValue({ records: [] });
    compareInGame.mockResolvedValue({ comparisons: [] });
    // The launches live in the store of the tab, so they have to be
    // emptied between the tests.
    useGameStore.setState({ launches: [] });
  });

  it("shows the share of the launcher in the launches of every version", async () => {
    useGameStore.setState({
      launches: [
        { versionId: "26.3", prepareMs: 700, at: 2 },
        { versionId: "26.3", prepareMs: 900, at: 1 },
        { versionId: "1.12.2", prepareMs: 1200, at: 0 },
      ],
    });
    renderScreen();

    expect(await screen.findByText("Доля лаунчера в запуске")).toBeInTheDocument();
    expect(screen.getByText("1.12.2")).toBeInTheDocument();
    // The middle of the two preparations of 26.3, and the slowest of
    // them.
    expect(screen.getByText("800 мс")).toBeInTheDocument();
    expect(screen.getByText("900 мс")).toBeInTheDocument();
  });

  it("says that there is nothing to show before the first launch", async () => {
    renderScreen();

    expect(await screen.findByText(/Появится после первого запуска/)).toBeInTheDocument();
  });

  it("shows the pair of the game when both of its runs were measured", async () => {
    compareInGame.mockResolvedValue({
      comparisons: [
        {
          metric: "in_game_fps",
          unit: "fps",
          optimized_scenario: "in_game/max_fps",
          optimized: 120,
          optimized_samples: 30,
          reference_scenario: "in_game/tlauncher",
          reference: 60,
          reference_samples: 30,
          improvement_percent: 100,
          verdict: "met",
        },
      ],
    });
    renderScreen();

    expect(await screen.findByText("FPS в игре")).toBeInTheDocument();
    expect(screen.getAllByText("60,0")).toHaveLength(2);
    expect(screen.getAllByText("120,0")).toHaveLength(2);
    expect(screen.getByText("+100,0 %")).toBeInTheDocument();
    expect(screen.getByText("Цель достигнута")).toBeInTheDocument();
  });

  it("shows nothing about the pair when only one of the runs was made", async () => {
    // A run with the mod alone has no base to be compared against, so
    // there is no pair to show.
    compareInGame.mockResolvedValue({ comparisons: [] });
    renderScreen();

    expect(await screen.findByText(/Сравнение появится после импорта эталона/)).toBeInTheDocument();
    expect(screen.queryByText("+100,0 %")).not.toBeInTheDocument();
  });

  it("shows the stored measurements of every scenario", async () => {
    latestBenchmarks.mockResolvedValue({
      records: [
        {
          suite: "game",
          metric: "in_game_fps",
          scenario: "in_game/off",
          samples: 1,
          p50: 60,
          p95: 60,
          p99: 60,
          mean: 60,
          stddev: 0,
          unit: "fps",
          created_at: "2026-10-20T10:00:00Z",
        },
        {
          suite: "game",
          metric: "in_game_fps",
          scenario: "in_game/balanced",
          samples: 1,
          p50: 120,
          p95: 120,
          p99: 120,
          mean: 120,
          stddev: 0,
          unit: "fps",
          created_at: "2026-10-20T10:00:00Z",
        },
      ],
    });
    renderScreen();

    expect(await screen.findByText("in_game/off")).toBeInTheDocument();
    expect(screen.getByText("in_game/balanced")).toBeInTheDocument();
  });

  it("says that there are no measurements yet", async () => {
    renderScreen();

    expect(await screen.findByText("Замеров пока нет")).toBeInTheDocument();
  });
});
