/**
 * Vue Statistiques d'écoute : temps total écouté (global, par mois, par podcast),
 * nombre d'épisodes terminés et temps économisé grâce à la vitesse accélérée.
 */
import { api } from "../api";
import { formatHumanTime, t } from "../i18n";
import { escapeHtml } from "../state";

export async function renderStatsView(): Promise<void> {
  const center = document.getElementById("center-pane");
  if (!center) return;

  const stats = await api.getStats();
  const maxMonth = Math.max(1, ...stats.by_month.map((m) => m.listened_ms));
  const maxFeed = Math.max(1, ...stats.by_feed.map((f) => f.listened_ms));

  center.innerHTML = `
    <div class="view-header">
      <div class="view-header-top">
        <div class="view-title">◔ ${escapeHtml(t("stats.title"))}</div>
      </div>
    </div>

    <div class="page-scroll">
      <div class="cards-grid" style="margin-top:0;margin-bottom:18px;">
        <div class="card">
          <div style="color:var(--text-secondary);font-size:12px;">${escapeHtml(
            t("stats.totalListened")
          )}</div>
          <div class="stat-value tabular">${escapeHtml(
            formatHumanTime(stats.total_listened_ms)
          )}</div>
        </div>
        <div class="card">
          <div style="color:var(--text-secondary);font-size:12px;">${escapeHtml(
            t("stats.timeSaved")
          )}</div>
          <div class="stat-value tabular">${escapeHtml(formatHumanTime(stats.total_saved_ms))}</div>
        </div>
        <div class="card">
          <div style="color:var(--text-secondary);font-size:12px;">${escapeHtml(
            t("stats.completedEpisodes")
          )}</div>
          <div class="stat-value tabular">${stats.completed_episodes}</div>
        </div>
      </div>

      <div class="card" style="margin-bottom:16px;">
        <div style="font-weight:700;margin-bottom:6px;">${escapeHtml(t("stats.byMonth"))}</div>
        ${
          stats.by_month.length === 0
            ? `<p style="color:var(--text-muted);">${escapeHtml(t("stats.noStats"))}</p>`
            : stats.by_month
                .map(
                  (m) => `
              <div class="bar-row">
                <span class="tabular">${escapeHtml(m.month)}</span>
                <div class="bar-track"><div class="bar-fill" style="width:${Math.round(
                  (m.listened_ms / maxMonth) * 100
                )}%"></div></div>
                <span class="tabular" style="text-align:right;color:var(--text-secondary);">${escapeHtml(
                  formatHumanTime(m.listened_ms)
                )}</span>
              </div>`
                )
                .join("")
        }
      </div>

      <div class="card">
        <div style="font-weight:700;margin-bottom:6px;">${escapeHtml(t("stats.byPodcast"))}</div>
        ${
          stats.by_feed.length === 0
            ? `<p style="color:var(--text-muted);">${escapeHtml(t("stats.noStats"))}</p>`
            : stats.by_feed
                .map(
                  (f) => `
              <div class="bar-row">
                <span style="overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">${escapeHtml(
                  f.feed_title
                )}</span>
                <div class="bar-track"><div class="bar-fill" style="width:${Math.round(
                  (f.listened_ms / maxFeed) * 100
                )}%"></div></div>
                <span class="tabular" style="text-align:right;color:var(--text-secondary);">${escapeHtml(
                  formatHumanTime(f.listened_ms)
                )}</span>
              </div>`
                )
                .join("")
        }
      </div>
    </div>
  `;
}
