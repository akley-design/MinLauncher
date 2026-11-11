import { useState } from "react";
import { parseActivityProgress, useActivityProgress } from "@minlauncher/activity-progress";

import { launcherAuth } from "../auth";
import { ATLAS_RELICS } from "../lib/atlas-relics";

const MAX_EQUIPPED = 4;

export function ActivityAtlasLoadout() {
  const { status, progress, userId } = useActivityProgress();
  const [savedSelection, setSavedSelection] = useState<{
    ownerId: string;
    serverSelection: string;
    ids: string[];
  } | null>(null);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState("");
  const serverSelection = progress?.equippedAtlasIds.join("|") ?? "";
  const equipped =
    savedSelection?.ownerId === userId && savedSelection.serverSelection === serverSelection
      ? savedSelection.ids
      : (progress?.equippedAtlasIds ?? []);
  const unlockedIds = new Set(progress?.unlockedRewardIds ?? []);
  const visibleEquipped = equipped.filter((id) => unlockedIds.has(id));

  async function toggleRelic(rewardId: string) {
    if (!progress || !userId || !launcherAuth || saving || !unlockedIds.has(rewardId)) return;

    const next = visibleEquipped.includes(rewardId)
      ? visibleEquipped.filter((id) => id !== rewardId)
      : [...visibleEquipped, rewardId];
    if (next.length > MAX_EQUIPPED) {
      setMessage("Можно разместить не больше четырёх реликвий.");
      return;
    }

    setSaving(true);
    setMessage("");
    try {
      const { data, error } = await launcherAuth.functions.invoke("activity-progress", {
        body: { action: "set_loadout", reward_ids: next },
      });
      if (error) throw error;
      const updated = parseActivityProgress(data);
      if (!updated) throw new Error("Invalid atlas response");
      setSavedSelection({
        ownerId: userId,
        serverSelection,
        ids: updated.equippedAtlasIds,
      });
      setMessage("Реликвии сохранены в аккаунте.");
    } catch {
      setMessage("Не удалось сохранить. Проверьте соединение и попробуйте снова.");
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="home-atlas">
      <div className="home-atlas__marks" aria-hidden="true">
        {ATLAS_RELICS.filter((relic) => visibleEquipped.includes(relic.id)).map((relic, index) => (
          <span
            key={relic.id}
            className={`home-atlas__mark home-atlas__mark--${index + 1}`}
            title={relic.name}
          >
            {relic.mark}
          </span>
        ))}
      </div>
      <details className="home-atlas__details">
        <summary className="home-atlas__summary">
          <span>
            <span className="home-atlas__eyebrow">МАСТЕРСКАЯ МИРОВ</span>
            <strong>Реликвии</strong>
          </span>
          <span className="home-atlas__count">
            {visibleEquipped.length}/{MAX_EQUIPPED}
          </span>
        </summary>
        <section className="home-atlas__panel" aria-label="Коллекция реликвий">
          <header className="home-atlas__panel-head">
            <strong>Собери свой мир</strong>
            <span>Выбери до четырёх деталей</span>
          </header>
          {progress ? (
            <div className="home-atlas__items">
              {ATLAS_RELICS.map((relic) => {
                const unlocked = unlockedIds.has(relic.id);
                const selected = visibleEquipped.includes(relic.id);
                return (
                  <button
                    key={relic.id}
                    type="button"
                    className="home-atlas__item"
                    data-unlocked={unlocked}
                    data-selected={selected}
                    aria-pressed={selected}
                    disabled={!unlocked || saving || !launcherAuth}
                    onClick={() => void toggleRelic(relic.id)}
                  >
                    <span className="home-atlas__item-mark" aria-hidden="true">
                      {unlocked ? relic.mark : "⌑"}
                    </span>
                    <span className="home-atlas__item-copy">
                      <strong>{relic.name}</strong>
                      <small>{unlocked ? "Открыто" : `Откроется на уровне ${relic.level}`}</small>
                    </span>
                    <span className="home-atlas__item-check" aria-hidden="true">
                      {selected ? "✓" : unlocked ? "+" : "·"}
                    </span>
                  </button>
                );
              })}
            </div>
          ) : (
            <p className="home-atlas__empty">
              {status === "signed_out"
                ? "Войдите в аккаунт MinLauncher, чтобы открыть и сохранять реликвии."
                : "Прогресс Атласа появится, когда сервер восстановит соединение."}
            </p>
          )}
          {saving ? (
            <p className="home-atlas__feedback" role="status">
              Сохраняем выбор…
            </p>
          ) : null}
          {message ? (
            <p className="home-atlas__feedback" role="status">
              {message}
            </p>
          ) : null}
          {progress ? (
            <p className="home-atlas__footnote">
              Уровень {progress.level}/50 · синхронизируется с аккаунтом сайта
            </p>
          ) : null}
        </section>
      </details>
    </div>
  );
}
