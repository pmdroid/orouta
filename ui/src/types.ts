export type Tps = {
  model: string
  avg: number
  last: number
  prompt: number | null
  samples: number
}

export type VramModel = {
  name: string
  size_vram: number
}

export type Vram = {
  loaded_bytes: number
  models: VramModel[]
}

export type Host = {
  id: string
  base_url: string
  disabled: boolean
  api_key_set: boolean
  reachable: boolean
  latency_ms: number
  models: string[]
  requests_total: number
  errors_total: number
  in_flight: number
  last_error: string | null
  tps: Tps[]
  vram: Vram | null
}

export type Tailscale = {
  self: string
  tailnet: string | null
  online: boolean
  serving: boolean
  url: string | null
}

export type StatusData = {
  hosts: Host[]
  tailscale: Tailscale | null
}

export type KeyView = {
  id: string
  label: string
  prefix: string
  created: string
  last_used: string
  last: boolean
}

export function gb(bytes: number): string {
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`
}

function stripLatest(model: string): string {
  return model.replace(/:latest$/, "")
}

export function tpsFor(host: Host, model: string): Tps | undefined {
  const m = stripLatest(model)
  return host.tps.find((t) => stripLatest(t.model) === m)
}
