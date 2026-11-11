// @vitest-environment jsdom
import { act, cleanup, render, screen } from "@testing-library/react";
import type {
  AuthChangeEvent,
  Session,
  SupabaseClient,
} from "@supabase/supabase-js";
import { afterEach, describe, expect, it, vi } from "vitest";

import {
  ACTIVITY_HEARTBEAT_INTERVAL_MS,
  ActivityProgressSummary,
  ActivityProgressProvider,
  useActivityProgress,
} from "@minlauncher/activity-progress";
import { ACTIVITY_CURVES } from "../../../supabase/functions/_shared/activity-levels.mjs";

const USER_ID = "7b7c68b1-12d1-4f7e-a299-a89c63250b31";
const SESSION_ID = "45d615bb-d572-41cd-a77d-0b46d902a331";
const curve = ACTIVITY_CURVES[1];
const firstLevelThresholdSeconds = curve[0]?.totalSeconds;
if (firstLevelThresholdSeconds === undefined) {
  throw new Error("Activity curve is missing level 1");
}

function progress(totalSeconds = 0, creditedMilliseconds = 0) {
  let level = 0;
  for (const entry of curve) {
    if (totalSeconds < entry.totalSeconds) break;
    level = entry.level;
  }
  const previous = level > 0 ? (curve[level - 1] ?? null) : null;
  const next = curve[level] ?? null;
  const nextReward = next;
  return {
    curveVersion: 1,
    level,
    totalSeconds,
    totalMilliseconds: totalSeconds * 1000,
    confirmedSeconds: totalSeconds,
    secondsIntoLevel: next ? totalSeconds - (previous?.totalSeconds ?? 0) : 0,
    requiredSeconds: next
      ? next.totalSeconds - (previous?.totalSeconds ?? 0)
      : 0,
    secondsRemaining: next ? next.totalSeconds - totalSeconds : 0,
    nextLevel: next?.level ?? null,
    unlockedRewardIds: curve.slice(0, level).map((entry) => entry.rewardId),
    creditedMilliseconds,
    currentReward: previous,
    nextReward,
  };
}

function makeClient() {
  let authListener:
    ((event: AuthChangeEvent, session: Session | null) => void) | undefined;
  const invoke = vi.fn(
    async (_name: string, options?: { body?: { action?: string } }) => {
      const action = options?.body?.action;
      if (action === "start")
        return { data: { sessionId: SESSION_ID }, error: null };
      if (action === "pause") return { data: { paused: true }, error: null };
      if (action === "heartbeat") {
        return {
          data: progress(firstLevelThresholdSeconds, 20_000),
          error: null,
        };
      }
      return { data: progress(), error: null };
    },
  );
  const client = {
    auth: {
      onAuthStateChange(callback: typeof authListener) {
        authListener = callback;
        return { data: { subscription: { unsubscribe: vi.fn() } } };
      },
      async getSession() {
        return {
          data: {
            session: {
              user: { id: USER_ID },
            } as Session,
          },
          error: null,
        };
      },
    },
    functions: { invoke },
  } as unknown as SupabaseClient;
  return {
    client,
    invoke,
    emitAuth: (session: Session | null) =>
      authListener?.("SIGNED_OUT", session),
  };
}

function Consumer() {
  const { status, progress: value } = useActivityProgress();
  return (
    <output data-testid="activity-state">
      {`${status}:${value?.level ?? "none"}`}
    </output>
  );
}

async function flushPromises() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
  });
}

describe("site activity tracking", () => {
  let visible = true;
  let focused = true;

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    visible = true;
    focused = true;
  });

  it("counts only while visible and focused, then starts a new session on return", async () => {
    vi.useFakeTimers();
    vi.spyOn(document, "hasFocus").mockImplementation(() => focused);
    vi.stubGlobal("navigator", { ...navigator, onLine: true });
    Object.defineProperty(document, "visibilityState", {
      configurable: true,
      get: () => (visible ? "visible" : "hidden"),
    });

    const { client, invoke } = makeClient();
    render(
      <ActivityProgressProvider client={client} source="site">
        <ActivityProgressSummary />
        <Consumer />
      </ActivityProgressProvider>,
    );
    await flushPromises();

    expect(
      invoke.mock.calls.some(
        ([, options]) => options?.body?.action === "start",
      ),
    ).toBe(true);
    expect(screen.getByTestId("activity-state").textContent).toContain(
      "active:0",
    );

    await act(async () => {
      vi.advanceTimersByTime(ACTIVITY_HEARTBEAT_INTERVAL_MS);
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(
      invoke.mock.calls.filter(
        ([, options]) => options?.body?.action === "heartbeat",
      ),
    ).toHaveLength(1);
    expect(screen.getByTestId("activity-state").textContent).toContain(
      "active:1",
    );
    expect(
      screen.getByLabelText("Прогресс Атласа миров").textContent,
    ).toContain("Рамка профиля «Новичок»");
    expect(screen.getByRole("progressbar").getAttribute("aria-valuenow")).toBe(
      "0",
    );

    visible = false;
    focused = false;
    await act(async () => {
      document.dispatchEvent(new Event("visibilitychange"));
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(
      invoke.mock.calls.filter(
        ([, options]) => options?.body?.action === "pause",
      ),
    ).toHaveLength(1);

    await act(async () => {
      vi.advanceTimersByTime(ACTIVITY_HEARTBEAT_INTERVAL_MS * 3);
    });
    expect(
      invoke.mock.calls.filter(
        ([, options]) => options?.body?.action === "heartbeat",
      ),
    ).toHaveLength(1);

    visible = true;
    focused = true;
    await act(async () => {
      document.dispatchEvent(new Event("visibilitychange"));
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(
      invoke.mock.calls.filter(
        ([, options]) => options?.body?.action === "start",
      ),
    ).toHaveLength(2);
  });

  it("does not start in a background tab and pauses when the account signs out", async () => {
    vi.useFakeTimers();
    visible = false;
    focused = false;
    vi.spyOn(document, "hasFocus").mockImplementation(() => focused);
    vi.stubGlobal("navigator", { ...navigator, onLine: true });
    Object.defineProperty(document, "visibilityState", {
      configurable: true,
      get: () => (visible ? "visible" : "hidden"),
    });

    const { client, invoke, emitAuth } = makeClient();
    render(
      <ActivityProgressProvider client={client} source="site">
        <Consumer />
      </ActivityProgressProvider>,
    );
    await flushPromises();
    expect(
      invoke.mock.calls.some(
        ([, options]) => options?.body?.action === "start",
      ),
    ).toBe(false);

    visible = true;
    focused = true;
    await act(async () => {
      document.dispatchEvent(new Event("visibilitychange"));
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(
      invoke.mock.calls.filter(
        ([, options]) => options?.body?.action === "start",
      ),
    ).toHaveLength(1);

    await act(async () => {
      emitAuth(null);
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(
      invoke.mock.calls.filter(
        ([, options]) => options?.body?.action === "pause",
      ),
    ).toHaveLength(1);
    expect(screen.getByTestId("activity-state").textContent).toContain(
      "signed_out:none",
    );
  });

  it("uses native window focus events for launcher activity", async () => {
    vi.useFakeTimers();
    focused = false;
    vi.spyOn(document, "hasFocus").mockImplementation(() => focused);
    vi.stubGlobal("navigator", { ...navigator, onLine: true });
    Object.defineProperty(document, "visibilityState", {
      configurable: true,
      get: () => "visible",
    });

    const { client, invoke } = makeClient();
    render(
      <ActivityProgressProvider client={client} source="launcher">
        <Consumer />
      </ActivityProgressProvider>,
    );
    await flushPromises();
    expect(
      invoke.mock.calls.some(
        ([, options]) => options?.body?.action === "start",
      ),
    ).toBe(false);

    await act(async () => {
      window.dispatchEvent(
        new CustomEvent("minlauncher:window-focus", { detail: true }),
      );
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(
      invoke.mock.calls.filter(
        ([, options]) => options?.body?.action === "start",
      ),
    ).toHaveLength(1);

    await act(async () => {
      window.dispatchEvent(
        new CustomEvent("minlauncher:window-focus", { detail: false }),
      );
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(
      invoke.mock.calls.filter(
        ([, options]) => options?.body?.action === "pause",
      ),
    ).toHaveLength(1);
  });

  it("pauses on browser-window blur even while the document stays visible", async () => {
    vi.useFakeTimers();
    vi.spyOn(document, "hasFocus").mockImplementation(() => focused);
    vi.stubGlobal("navigator", { ...navigator, onLine: true });

    const { client, invoke } = makeClient();
    render(
      <ActivityProgressProvider client={client} source="site">
        <Consumer />
      </ActivityProgressProvider>,
    );
    await flushPromises();

    focused = false;
    await act(async () => {
      window.dispatchEvent(new Event("blur"));
      await flushPromises();
    });
    expect(document.visibilityState).toBe("visible");
    expect(
      invoke.mock.calls.filter(
        ([, options]) => options?.body?.action === "pause",
      ),
    ).toHaveLength(1);

    await act(async () => {
      vi.advanceTimersByTime(ACTIVITY_HEARTBEAT_INTERVAL_MS * 3);
    });
    expect(
      invoke.mock.calls.filter(
        ([, options]) => options?.body?.action === "heartbeat",
      ),
    ).toHaveLength(0);

    focused = true;
    await act(async () => {
      window.dispatchEvent(new Event("focus"));
      await flushPromises();
    });
    expect(
      invoke.mock.calls.filter(
        ([, options]) => options?.body?.action === "start",
      ),
    ).toHaveLength(2);
  });

  it("pauses on pagehide and starts a fresh session when the page returns", async () => {
    vi.useFakeTimers();
    vi.spyOn(document, "hasFocus").mockImplementation(() => focused);
    vi.stubGlobal("navigator", { ...navigator, onLine: true });

    const { client, invoke } = makeClient();
    render(
      <ActivityProgressProvider client={client} source="site">
        <Consumer />
      </ActivityProgressProvider>,
    );
    await flushPromises();

    await act(async () => {
      window.dispatchEvent(new Event("pagehide"));
      await flushPromises();
    });
    expect(
      invoke.mock.calls.filter(
        ([, options]) => options?.body?.action === "pause",
      ),
    ).toHaveLength(1);

    await act(async () => {
      vi.advanceTimersByTime(ACTIVITY_HEARTBEAT_INTERVAL_MS * 3);
    });
    expect(
      invoke.mock.calls.filter(
        ([, options]) => options?.body?.action === "heartbeat",
      ),
    ).toHaveLength(0);

    await act(async () => {
      window.dispatchEvent(new Event("pageshow"));
      await flushPromises();
    });
    expect(
      invoke.mock.calls.filter(
        ([, options]) => options?.body?.action === "start",
      ),
    ).toHaveLength(2);
  });

  it("pauses offline and creates a new session after the connection returns", async () => {
    vi.useFakeTimers();
    vi.spyOn(document, "hasFocus").mockImplementation(() => focused);
    const network = { ...navigator, onLine: true };
    vi.stubGlobal("navigator", network);

    const { client, invoke } = makeClient();
    render(
      <ActivityProgressProvider client={client} source="site">
        <Consumer />
      </ActivityProgressProvider>,
    );
    await flushPromises();

    network.onLine = false;
    await act(async () => {
      window.dispatchEvent(new Event("offline"));
      await flushPromises();
    });
    expect(screen.getByTestId("activity-state").textContent).toContain(
      "offline:0",
    );
    expect(
      invoke.mock.calls.filter(
        ([, options]) => options?.body?.action === "pause",
      ),
    ).toHaveLength(1);

    await act(async () => {
      vi.advanceTimersByTime(ACTIVITY_HEARTBEAT_INTERVAL_MS * 3);
    });
    expect(
      invoke.mock.calls.filter(
        ([, options]) => options?.body?.action === "heartbeat",
      ),
    ).toHaveLength(0);

    network.onLine = true;
    await act(async () => {
      window.dispatchEvent(new Event("online"));
      await flushPromises();
    });
    expect(
      invoke.mock.calls.filter(
        ([, options]) => options?.body?.action === "start",
      ),
    ).toHaveLength(2);
  });
});
