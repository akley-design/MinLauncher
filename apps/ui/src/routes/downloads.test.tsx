/**
 * The screen of the download queue.
 *
 * The screen is a view of the queue and nothing else: it shows what the
 * backend wrote and it sends the commands the player asked for. These
 * tests hold that line — the table shows the entries, the button of a
 * failed entry asks the backend to fetch it again, and the clearing
 * buttons ask to clear the right half of the queue.
 */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { DownloadEntry } from "../ipc/types";
import { DownloadsRoute } from "./downloads";

// The backend is replaced by a mock, so that the test does not need a
// running desktop application.
const listDownloads = vi.fn<() => Promise<DownloadEntry[]>>();
const retryDownload = vi.fn<(url: string) => Promise<DownloadEntry>>();
const clearCompletedDownloads = vi.fn<() => Promise<number>>();
const clearFailedDownloads = vi.fn<() => Promise<number>>();

vi.mock("../ipc/client", async () => {
  const actual = await vi.importActual<typeof import("../ipc/client")>("../ipc/client");
  return {
    ...actual,
    listDownloads: () => listDownloads(),
    retryDownload: (url: string) => retryDownload(url),
    clearCompletedDownloads: () => clearCompletedDownloads(),
    clearFailedDownloads: () => clearFailedDownloads(),
  };
});

/** One entry of the queue. */
function entry(overrides: Partial<DownloadEntry> = {}): DownloadEntry {
  return {
    url: "https://example.invalid/library.jar",
    sha1: "abc",
    size: 1024,
    target_path: "C:/Users/player/.minecraft/library.jar",
    state: "completed",
    bytes_done: 1024,
    attempts: 1,
    error: null,
    updated_at: "2026-10-20T10:00:00Z",
    ...overrides,
  };
}

/** The screen, with a fresh query cache around it. */
function renderScreen() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });

  return render(
    <QueryClientProvider client={client}>
      <DownloadsRoute />
    </QueryClientProvider>,
  );
}

describe("the screen of the download queue", () => {
  beforeEach(() => {
    listDownloads.mockReset();
    retryDownload.mockReset();
    clearCompletedDownloads.mockReset();
    clearFailedDownloads.mockReset();
    clearCompletedDownloads.mockResolvedValue(1);
    clearFailedDownloads.mockResolvedValue(1);
  });

  it("shows the entries of the queue", async () => {
    listDownloads.mockResolvedValue([
      entry(),
      entry({
        url: "https://example.invalid/broken.jar",
        target_path: "C:/Users/player/.minecraft/broken.jar",
        state: "failed",
        bytes_done: 0,
        attempts: 4,
        error: "the network is down",
      }),
    ]);
    renderScreen();

    expect(await screen.findByText("library.jar")).toBeInTheDocument();
    expect(screen.getByText("broken.jar")).toBeInTheDocument();
    expect(screen.getByText("Готово")).toBeInTheDocument();
    expect(screen.getByText("Ошибка")).toBeInTheDocument();
  });

  it("shows the reason of a failure", async () => {
    listDownloads.mockResolvedValue([entry({ state: "failed", error: "the network is down" })]);
    renderScreen();

    expect(await screen.findByText("1 записей, из них неудачных — 1")).toBeInTheDocument();
  });

  it("asks the backend to fetch a failed entry again", async () => {
    const failed = entry({
      url: "https://example.invalid/broken.jar",
      target_path: "C:/Users/player/.minecraft/broken.jar",
      state: "failed",
    });
    listDownloads.mockResolvedValue([failed]);
    retryDownload.mockResolvedValue(entry());

    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "Повторить" }));

    await waitFor(() => expect(retryDownload).toHaveBeenCalledTimes(1));
    expect(retryDownload.mock.calls[0]?.[0]).toBe("https://example.invalid/broken.jar");
  });

  it("offers a retry only to the entries that failed", async () => {
    listDownloads.mockResolvedValue([entry()]);
    renderScreen();

    expect(await screen.findByText("library.jar")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Повторить" })).not.toBeInTheDocument();
  });

  it("clears the finished and the failed entries separately", async () => {
    listDownloads.mockResolvedValue([
      entry(),
      entry({ url: "https://example.invalid/broken.jar", state: "failed" }),
    ]);
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "Очистить завершённые" }));
    await waitFor(() => expect(clearCompletedDownloads).toHaveBeenCalledTimes(1));

    await userEvent.click(screen.getByRole("button", { name: "Очистить неудачные" }));
    await waitFor(() => expect(clearFailedDownloads).toHaveBeenCalledTimes(1));
  });

  it("says that the queue is empty when there is nothing in it", async () => {
    listDownloads.mockResolvedValue([]);
    renderScreen();

    expect(await screen.findByText("Очередь пуста")).toBeInTheDocument();
  });

  it("shows the failure of the backend instead of the table", async () => {
    listDownloads.mockRejectedValue({
      code: "storage",
      message: "the database is locked",
    });
    renderScreen();

    expect(await screen.findByText("the database is locked")).toBeInTheDocument();
  });
});
