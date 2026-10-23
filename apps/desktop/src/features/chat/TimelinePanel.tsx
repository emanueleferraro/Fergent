/**
 * Time travel (Task 2.19): recent file backups and a reversible restore.
 */
import { useCallback, useEffect, useState, type ReactElement } from 'react';
import type { Backup, BackupListResult, BackupRestoreResult } from '@fergents/protocol';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';

/** Props for `TimelinePanel`. */
export interface TimelinePanelProps {
  kernel: KernelClient;
}

/** List recent backups and restore one. */
export function TimelinePanel({ kernel }: TimelinePanelProps): ReactElement {
  const [backups, setBackups] = useState<Backup[]>([]);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async (): Promise<void> => {
    try {
      const result = await kernel.call<BackupListResult>('backup.list', { limit: 20 });
      setBackups(result.backups);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  }, [kernel]);

  useEffect(() => {
    void load();
  }, [load]);

  const restore = async (id: string): Promise<void> => {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const result = await kernel.call<BackupRestoreResult>('backup.restore', { backup_id: id });
      setNotice(t('timeline.restored', { path: result.restored }));
      await load();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="panel panel-stack" aria-label={t('timeline.title')}>
      <div className="panel-title">{t('timeline.title')}</div>
      <p className="t-caption">{t('timeline.desc')}</p>

      {error ? <div className="inline-feedback error">{error}</div> : null}
      {notice ? <div className="inline-feedback info">{notice}</div> : null}

      <div className="panel-row">
        <button type="button" className="btn btn-sm" disabled={busy} onClick={() => void load()}>
          {t('timeline.refresh')}
        </button>
      </div>

      {backups.length === 0 ? (
        <div className="inline-feedback info">{t('timeline.empty')}</div>
      ) : (
        <ul className="timeline">
          {backups.map((backup) => (
            <li key={backup.id} className="timeline__row">
              <div>
                <div className="timeline__path">{backup.path}</div>
                <div className="t-caption">{new Date(backup.created_at).toLocaleString()}</div>
              </div>
              <button
                type="button"
                className="btn btn-sm"
                disabled={busy}
                onClick={() => void restore(backup.id)}
              >
                {t('timeline.restore')}
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
