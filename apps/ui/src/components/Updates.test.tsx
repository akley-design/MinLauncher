/**
 * The section of the settings that updates the launcher itself.
 *
 * The three states of the updater are told apart here: there is no
 * server to ask, the launcher is the newest one, and a newer version is
 * waiting. The third one is the only one that offers the button of the
 * installation — and the manifest of the release has to travel back to
 * the backend with the request, so that the version that is installed is
 * the one that was shown.
 */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { UpdateView } from "../ipc/types";
import { Updates } from "./Updates";

// The backend is replaced by a mock, so that the test does not need a
// running desktop application.
const checkForUpdate = vi.fn<() => Promise<UpdateView>>();
const installUpdate = vi.fn<(rawJson: unknown) => Promise<void>>();

vi.mock("../ipc/client", async () => {
  const actual = await vi.importActual<typeof import("../ipc/client")>("../ipc/client");
  return {
    ...actual,
    checkForUpdate: () => checkForUpdate(),
    installUpdate: (rawJson: unknown) => installUpdate(rawJson),
  };
});

/** The answer of the backend: no server to ask. */
function noServer(): UpdateView {
  return {
    configured: false,
    available: false,
    current_version: "0.1.0",
    version: null,
    notes: null,
    raw_json: null,
  };
}

/** The answer of the backend: the launcher is the newest one. */
function upToDate(): UpdateView {
  return {
    configured: true,
    available: false,
    current_version: "0.1.0",
    version: null,
    notes: null,
    raw_json: null,
  };
}

/** The answer of the backend: a newer version is waiting. */
function newer(): UpdateView {
  return {
    configured: true,
    available: true,
    current_version: "0.1.0",
    version: "0.2.0",
    notes: "Faster.",
    raw_json: { version: "0.2.0" },
  };
}

/** The section, with a fresh query cache around it. */
function renderSection() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });

  return render(
    <QueryClientProvider client={client}>
      <Updates />
    </QueryClientProvider>,
  );
}

describe("the section of the updates", () => {
  beforeEach(() => {
    checkForUpdate.mockReset();
    installUpdate.mockReset();
    installUpdate.mockResolvedValue(undefined);
  });

  it("says so when there is no server of the updates", async () => {
    checkForUpdate.mockResolvedValue(noServer());
    renderSection();

    await userEvent.click(screen.getByRole("button", { name: "Проверить обновления" }));

    expect(await screen.findByText("Сервер обновлений не настроен.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Скачать и установить" })).not.toBeInTheDocument();
  });

  it("says so when the launcher is the newest one", async () => {
    checkForUpdate.mockResolvedValue(upToDate());
    renderSection();

    await userEvent.click(screen.getByRole("button", { name: "Проверить обновления" }));

    expect(await screen.findByText(/0\.1\.0 — самая свежая/)).toBeInTheDocument();
  });

  it("offers the newer version and sends its manifest back", async () => {
    checkForUpdate.mockResolvedValue(newer());
    renderSection();

    await userEvent.click(screen.getByRole("button", { name: "Проверить обновления" }));

    expect(await screen.findByText("0.2.0")).toBeInTheDocument();
    expect(screen.getByText("Faster.")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Скачать и установить" }));

    await waitFor(() => expect(installUpdate).toHaveBeenCalledTimes(1));
    expect(installUpdate.mock.calls[0]?.[0]).toEqual({ version: "0.2.0" });
  });

  it("shows the failure of the check instead of a version", async () => {
    checkForUpdate.mockRejectedValue({
      code: "network",
      message: "the server of the updates is not answering",
    });
    renderSection();

    await userEvent.click(screen.getByRole("button", { name: "Проверить обновления" }));

    expect(
      await screen.findByText("the server of the updates is not answering"),
    ).toBeInTheDocument();
  });
});
