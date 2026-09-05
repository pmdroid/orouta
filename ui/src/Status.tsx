import { useCallback, useEffect, useState, type FormEvent } from "react"
import { ApiError, del, getJson, postJson } from "./api"
import Header, { TsChip } from "./Header"
import { type Host, type KeyView, type StatusData, gb, tpsFor } from "./types"

const POLL_MS = 5000

type Props = {
  onUnauthorized: () => void
}

export default function Status({ onUnauthorized }: Props) {
  const [data, setData] = useState<StatusData | null>(null)
  const [updated, setUpdated] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [manageable, setManageable] = useState(false)

  const fail = useCallback(
    (e: unknown) => {
      if (e instanceof ApiError && e.status === 401) {
        onUnauthorized()
        return
      }
      setError(e instanceof Error ? e.message : String(e))
    },
    [onUnauthorized],
  )

  const load = useCallback(async () => {
    try {
      const d = await getJson<StatusData>("/status.json")
      setData(d)
      setUpdated(new Date().toLocaleTimeString())
      setError(null)
    } catch (e) {
      fail(e)
    }
  }, [fail])

  useEffect(() => {
    getJson<{ keys: KeyView[] }>("/api/keys")
      .then((v) => setManageable(v.keys.length > 0))
      .catch((e) => {
        if (e instanceof ApiError && e.status === 401) onUnauthorized()
      })
  }, [onUnauthorized])

  useEffect(() => {
    load()
    const t = setInterval(load, POLL_MS)
    return () => clearInterval(t)
  }, [load])

  async function act(p: Promise<unknown>) {
    try {
      await p
      setError(null)
      await load()
    } catch (e) {
      fail(e)
    }
  }

  const hosts = data?.hosts ?? []
  const up = hosts.filter((h) => !h.disabled && h.reachable).length
  const models = hosts.reduce((n, h) => n + h.models.length, 0)
  const requests = hosts.reduce((n, h) => n + h.requests_total, 0)
  const errors = hosts.reduce((n, h) => n + h.errors_total, 0)
  const inFlight = hosts.reduce((n, h) => n + h.in_flight, 0)

  return (
    <div className="wrap">
      <Header
        title="/ status"
        active="status"
        sub={
          <>
            {hosts.length} hosts · {updated ? `updated ${updated}` : "loading"} ·{" "}
            <a href="/status.json">JSON</a>
            <TsChip ts={data?.tailscale ?? null} />
          </>
        }
      />
      {error && <div className="error">{error}</div>}
      <div className="summary">
        <Stat n={`${up}/${hosts.length}`} label="hosts up" />
        <Stat n={models} label="models" />
        <Stat n={requests} label="requests" />
        <Stat n={errors} label="errors" />
        <Stat n={inFlight} label="in-flight" />
      </div>
      <table>
        <thead>
          <tr>
            <th>Host</th>
            <th>Reachable</th>
            <th className="num">Models</th>
            <th className="num">Requests</th>
            <th className="num">Errors</th>
            <th className="num">In-flight</th>
            <th>Last error</th>
            {manageable && <th>Actions</th>}
          </tr>
        </thead>
        <tbody>
          {hosts.map((h) => (
            <Row key={h.id} host={h} manageable={manageable} act={act} />
          ))}
        </tbody>
      </table>
      {manageable ? <AddHost act={act} /> : <p className="url locked">This proxy has no API keys configured, so it can't be administered remotely. Add keys under [auth].keys in orouta.toml to enable host management.</p>}
    </div>
  )
}

function Stat({ n, label }: { n: number | string; label: string }) {
  return (
    <div className="stat">
      <b>{n}</b>
      <small>{label}</small>
    </div>
  )
}

function Row({
  host,
  manageable,
  act,
}: {
  host: Host
  manageable: boolean
  act: (p: Promise<unknown>) => Promise<void>
}) {
  async function toggle() {
    const action = host.disabled ? "enable" : "disable"
    await act(postJson(`/api/hosts/${encodeURIComponent(host.id)}/${action}`))
  }
  async function remove() {
    if (!window.confirm(`Remove host ${host.id}?`)) return
    await act(del(`/api/hosts/${encodeURIComponent(host.id)}`))
  }
  const vram = host.vram
  return (
    <tr className={host.disabled ? "disabled" : undefined}>
      <td>
        <b>{host.id}</b>
        <br />
        <span className="url">{host.base_url}</span>
        <br />
        <span className="url">api_key: {host.api_key_set ? "set" : "unset"}</span>
        <br />
        {vram && <span className="url">vram: {gb(vram.loaded_bytes)}</span>}
        {vram && vram.models.length > 0 && (
          <span className="url">
            {" "}
            ({vram.models.map((m) => `${m.name} ${gb(m.size_vram)}`).join(", ")})
          </span>
        )}
      </td>
      <td>
        {host.disabled ? (
          <>
            <span className="disabled-tag">DISABLED</span>
            <br />
            <span className="url">not probed</span>
          </>
        ) : host.reachable ? (
          <>
            <span className="up">● up</span>
            <br />
            <span className="url">{host.latency_ms}ms</span>
          </>
        ) : (
          <>
            <span className="dot down" />
            <span className="down">down</span>
          </>
        )}
      </td>
      <td className="num" data-label="models">
        {host.models.length === 0 ? (
          <span className="url">—</span>
        ) : (
          host.models.map((m) => <ModelChip key={m} model={m} host={host} />)
        )}
      </td>
      <td className="num" data-label="requests">
        {host.requests_total}
      </td>
      <td className="num" data-label="errors">
        {host.errors_total}
      </td>
      <td className="num" data-label="in-flight">
        {host.in_flight}
      </td>
      <td>{host.last_error ? <span className="err">{host.last_error}</span> : <span className="url">—</span>}</td>
      {manageable && (
        <td>
          <span className="actions">
            <label className="switch">
              <input type="checkbox" checked={!host.disabled} onChange={toggle} />
              <span className="slider" />
            </label>
            <button className="remove" onClick={remove}>
              remove
            </button>
          </span>
        </td>
      )}
    </tr>
  )
}

function ModelChip({ model, host }: { model: string; host: Host }) {
  const t = tpsFor(host, model)
  return (
    <span className="model">
      {model}{" "}
      {t && (
        <span className="tps">
          ~{t.avg.toFixed(1)} tok/s
          {t.prompt !== null && <> · prompt {t.prompt.toFixed(1)}</>} · {t.samples} sample
          {t.samples === 1 ? "" : "s"}
        </span>
      )}
    </span>
  )
}

function AddHost({ act }: { act: (p: Promise<unknown>) => Promise<void> }) {
  const [id, setId] = useState("")
  const [baseUrl, setBaseUrl] = useState("")
  const [apiKey, setApiKey] = useState("")
  async function submit(e: FormEvent) {
    e.preventDefault()
    const body: Record<string, string> = { id, base_url: baseUrl }
    if (apiKey) body.api_key = apiKey
    await act(postJson("/api/hosts", body))
    setId("")
    setBaseUrl("")
    setApiKey("")
  }
  return (
    <div className="add">
      <h2>Add host</h2>
      <form className="row" onSubmit={submit}>
        <label>
          id
          <input type="text" value={id} onChange={(e) => setId(e.target.value)} />
        </label>
        <label>
          base_url
          <input type="text" value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} />
        </label>
        <label>
          api_key (optional)
          <input type="password" value={apiKey} onChange={(e) => setApiKey(e.target.value)} />
        </label>
        <button className="btn" type="submit">
          Add
        </button>
      </form>
    </div>
  )
}
