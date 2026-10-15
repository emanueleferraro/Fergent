// @generated — do not edit
// Generated from protocol/schema/protocol.schema.json by `pnpm --filter @fergents/protocol gen`.

/**
 * A kernel event emitted on the bus (ARCHITECTURE.md §4.2).
 */
export interface Event {
  /**
   * ULID, monotonic.
   */
  id: string;
  /**
   * Milliseconds since the Unix epoch.
   */
  ts: number;
  /**
   * Namespaced topic, e.g. kernel.started.
   */
  kind: string;
  /**
   * Who emitted the event, e.g. agent:ferg-01.
   */
  source: string;
  /**
   * Groups events in one logical operation.
   */
  correlation_id?: string | null;
  /**
   * Optional brain scope.
   */
  brain_id?: string | null;
  /**
   * Optional project scope.
   */
  project_id?: string | null;
  /**
   * Topic-specific payload (arbitrary JSON).
   */
  payload: {
    [k: string]: unknown;
  };
  /**
   * Event schema version.
   */
  version: number;
}

export type HealthStatus = 'healthy' | 'degraded' | 'unhealthy';

export type StorageStatus = 'ok' | 'readonly' | 'error';

/**
 * Result of the system.health command.
 */
export interface HealthSnapshot {
  status: HealthStatus;
  uptime_ms: number;
  kernel_version: string;
  protocol_version: number;
  storage: StorageStatus;
}

/**
 * First command sent by a client after connecting.
 */
export interface HelloParams {
  protocol_version: number;
  /**
   * desktop | cli | mobile | mcp
   */
  client_kind: string;
  client_version?: string | null;
  capabilities?: string[] | null;
}

/**
 * Kernel reply to hello when the client version is compatible.
 */
export interface WelcomeResult {
  protocol_version: number;
  kernel_version: string;
  session_id: string;
  server_capabilities?: string[] | null;
}

/**
 * Liveness probe reply; carries no diagnostics.
 */
export interface PingResult {
  pong: boolean;
  ts: number;
}

export interface SystemInfoResult {
  kernel_version: string;
  protocol_version: number;
  os: string;
  pid: number;
}

export interface SubscribeParams {
  /**
   * Topic prefixes, e.g. kernel. or file.changed.
   */
  topics: string[];
}

export interface UnsubscribeParams {
  topics: string[];
}

export interface ReplayParams {
  topics: string[];
  /**
   * Deliver events strictly after this id.
   */
  from_id?: string | null;
  limit?: number | null;
}

export interface BrainSummary {
  brain_id: string;
  name: string;
  path: string;
  role?: string | null;
}

export interface BrainListResult {
  brains: BrainSummary[];
}

/**
 * Payload of the events.dropped notice.
 */
export interface EventsDroppedNotice {
  dropped: number;
  last_seen_id: string | null;
}
