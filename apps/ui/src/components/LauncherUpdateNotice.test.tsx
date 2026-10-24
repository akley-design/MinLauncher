import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { UpdateView } from "../ipc/types";
import { LauncherUpdateNotice } from "./LauncherUpdateNotice";

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

function answer(available: boolean): UpdateView {
  return {
    configured: true,
    available,
    current_version: "0.1.0",
    version: available ? "0.2.0" : null,
    notes: available ? "Исправления и улучшения" : null,
    raw_json: available ? { version: "0.2.0", signature: "signed" } : null,
  };
}

function renderNotice() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
  return render(
    <QueryClientProvider client={client}>
      <LauncherUpdateNotice />
    </QueryClientProvider>,
  );
}

describe("launcher update notice", () => {
  beforeEach(() => {
    checkForUpdate.mockReset();
    installUpdate.mockReset();
    installUpdate.mockResolvedValue(undefined);
  });

  it("checks for updates on mount and offers installation when one is available", async () => {
    checkForUpdate.mockResolvedValue(answer(true));

    renderNotice();

    expect(await screen.findByText("Доступно обновление MinLauncher 0.2.0"))
      .toBeInTheDocument();
    expect(checkForUpdate).toHaveBeenCalledTimes(1);
    await userEvent.click(screen.getByRole("button", { name: "Обновить сейчас" }));

    await waitFor(() => expect(installUpdate).toHaveBeenCalledWith(answer(true).raw_json));
  });

  it("does not show a notice when the launcher is up to date", async () => {
    checkForUpdate.mockResolvedValue(answer(false));

    renderNotice();

    await waitFor(() => expect(checkForUpdate).toHaveBeenCalledTimes(1));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
});
