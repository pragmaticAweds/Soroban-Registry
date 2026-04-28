/**
 * Stellar Expert public API adapter
 * Docs: https://stellar.expert/openapi.html
 *
 * Used as a live data fallback when NEXT_PUBLIC_API_URL backend is unreachable
 * or when NEXT_PUBLIC_USE_MOCKS is not set.
 *
 * Exports:
 *  - fetchRecentContracts(network, limit) → Contract[]
 *  - fetchContractActivity(network, limit) → AnalyticsEvent[]
 */

import type { Contract, AnalyticsEvent } from "@/types";

export type StellarNetwork = "public" | "testnet";

// Map our internal network IDs to Stellar Expert path segments
const NETWORK_MAP: Record<string, StellarNetwork> = {
  mainnet: "public",
  testnet: "testnet",
  futurenet: "testnet", // futurenet not supported by Expert; fall back to testnet
};

const BASE = "https://api.stellar.expert";

// ─── Types from Stellar Expert API ──────────────────────────────────────────

interface ExpertContract {
  id: string;           // Soroban contract ID (C...)
  creator?: string;     // G... address
  created?: number;     // Unix epoch seconds
  wasm?: string;        // wasm hash
  payments?: number;
  trades?: number;
  asset_created?: number;
}

interface ExpertContractListResponse {
  _embedded: {
    records: ExpertContract[];
  };
  _links?: unknown;
}

interface ExpertOperationRecord {
  id: string;
  type: number;           // operation type code
  type_i?: number;
  paging_token: string;
  transaction_hash: string;
  source_account: string;
  created_at: string;     // ISO 8601
  // For contract invocations / uploads
  contract_id?: string;
  function?: string;
  // For account create / payments
  from?: string;
  to?: string;
  amount?: string;
  asset_type?: string;
}

interface ExpertOperationsResponse {
  _embedded: {
    records: ExpertOperationRecord[];
  };
}

// ─── Fetch helpers ────────────────────────────────────────────────────────────

async function expertGet<T>(path: string): Promise<T> {
  const url = `${BASE}${path}`;
  const res = await fetch(url, {
    headers: { Accept: "application/json" },
    // 10 s hard timeout
    signal: AbortSignal.timeout(10_000),
  });
  if (!res.ok) throw new Error(`Stellar Expert ${url}: HTTP ${res.status}`);
  return res.json() as Promise<T>;
}

// ─── Map Expert operation type codes to our event types ──────────────────────
// https://developers.stellar.org/docs/data/horizon/api-reference/resources/operations/object
// 24 = upload_contract_wasm, 25 = invoke_host_function (includes create/invoke)

function opTypeToEventType(
  typeCode: number,
  fn?: string,
): AnalyticsEvent["event_type"] {
  if (typeCode === 24) return "contract_published"; // UploadContractWasm
  if (typeCode === 25) {
    if (!fn) return "contract_deployed";
    if (fn.toLowerCase().includes("upgrade")) return "version_created";
    if (fn.toLowerCase().includes("verify")) return "contract_verified";
    return "contract_deployed";
  }
  return "contract_updated";
}

// ─── Public API ───────────────────────────────────────────────────────────────

/**
 * Fetch most-recently created Soroban contracts from Stellar Expert.
 * Returns them shaped as our internal Contract type.
 */
export async function fetchRecentContracts(
  network: string = "mainnet",
  limit = 20,
): Promise<Contract[]> {
  const net = NETWORK_MAP[network] ?? "public";
  const data = await expertGet<ExpertContractListResponse>(
    `/explorer/${net}/contract?limit=${limit}&order=desc`,
  );

  return data._embedded.records.map((r): Contract => ({
    // Required shape matching frontend/types/index.ts Contract
    id: r.id,
    contract_id: r.id,
    name: shortenId(r.id),
    description: null,
    network: network === "testnet" ? "testnet" : "mainnet",
    publisher_id: r.creator ?? "unknown",
    version: "1.0.0",
    category: null,
    tags: [],
    is_verified: false,
    is_deprecated: false,
    wasm_hash: r.wasm ?? null,
    source_code_url: null,
    documentation_url: null,
    repository_url: null,
    license: null,
    created_at: r.created
      ? new Date(r.created * 1000).toISOString()
      : new Date().toISOString(),
    updated_at: r.created
      ? new Date(r.created * 1000).toISOString()
      : new Date().toISOString(),
    popularity_score: (r.payments ?? 0) + (r.trades ?? 0),
    downloads: null,
    average_rating: null,
    avg_rating: null,
    review_count: 0,
  }));
}

/**
 * Fetch recent Soroban-related operations and shape them as AnalyticsEvents.
 */
export async function fetchContractActivity(
  network: string = "mainnet",
  limit = 30,
): Promise<AnalyticsEvent[]> {
  const net = NETWORK_MAP[network] ?? "public";

  // Stellar Expert operations endpoint — filter to contract-related op types
  const data = await expertGet<ExpertOperationsResponse>(
    `/explorer/${net}/operations?type=invoke_host_function&limit=${limit}&order=desc`,
  );

  return data._embedded.records.map((r): AnalyticsEvent => ({
    id: r.id,
    event_type: opTypeToEventType(r.type ?? 25, r.function),
    contract_id: r.contract_id ?? r.transaction_hash.slice(0, 16),
    user_address: r.source_account,
    network: network === "testnet" ? "testnet" : "mainnet",
    metadata: {
      name: r.contract_id ? shortenId(r.contract_id) : undefined,
      function: r.function,
      tx_hash: r.transaction_hash,
    },
    created_at: r.created_at,
  }));
}

// ─── Utility ──────────────────────────────────────────────────────────────────

function shortenId(id: string): string {
  if (id.length <= 12) return id;
  return `${id.slice(0, 6)}…${id.slice(-4)}`;
}
