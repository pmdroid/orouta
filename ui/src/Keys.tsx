import { useCallback, useEffect, useState, type FormEvent } from "react"
import { ApiError, del, getJson, postJson } from "./api"
import Header from "./Header"
import type { KeyView } from "./types"

type Props = {
  onUnauthorized: () => void
}

export default function Keys({ onUnauthorized }: Props) {
  const [keys, setKeys] = useState<KeyView[] | null>(null)
  const [label, setLabel] = useState("")
  const [reveal, setReveal] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)
  const [error, setError] = useState<string | null>(null)

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

  useEffect(() => {
    getJson<{ keys: KeyView[] }>("/api/keys")
      .then((v) => setKeys(v.keys))
      .catch(fail)
  }, [fail])

  async function create(e: FormEvent) {
    e.preventDefault()
    try {
      const body = label.trim() ? { label: label.trim() } : {}
      const v = await postJson<{ secret: string; keys: KeyView[] }>("/api/keys", body)
      setKeys(v.keys)
      setReveal(v.secret)
      setCopied(false)
      setLabel("")
      setError(null)
    } catch (err) {
      fail(err)
    }
  }

  async function revoke(k: KeyView) {
    if (k.last && !window.confirm("This is the last API key. Revoking it leaves the proxy without keys until you add one to orouta.toml. Revoke anyway?")) {
      return
    }
    try {
      const v = await del<{ keys: KeyView[] }>(`/api/keys/${encodeURIComponent(k.id)}`)
      setKeys(v.keys)
      setError(null)
    } catch (err) {
      fail(err)
    }
  }

  function copy() {
    if (reveal === null) return
    navigator.clipboard.writeText(reveal)
    setCopied(true)
  }

  return (
    <div className="wrap">
      <Header
        title="/ api keys"
        active="keys"
        sub={<>keys authorize everything the proxy can do · revoked keys stop working on the next request</>}
      />
      {error && <div className="error">{error}</div>}
      <div className="add">
        <h2>API keys</h2>
        {reveal && (
          <div className="reveal">
            <button className="btn" onClick={copy}>
              {copied ? "copied" : "copy"}
            </button>
            <b>New key created — copy it now, it won't be shown again</b>
            <code>{reveal}</code>
          </div>
        )}
        {keys !== null && keys.length > 0 && (
          <table>
            <thead>
              <tr>
                <th>Label</th>
                <th>Key</th>
                <th>Created</th>
                <th>Last used</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {keys.map((k) => (
                <tr key={k.id}>
                  <td>
                    <span className="klabel">{k.label}</span>
                  </td>
                  <td>
                    <span className="kprefix">{k.prefix}…</span>
                  </td>
                  <td>
                    <span className="ktime">{k.created}</span>
                  </td>
                  <td>
                    <span className="ktime">{k.last_used}</span>
                  </td>
                  <td>
                    <button className="revoke" onClick={() => revoke(k)}>
                      revoke
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        {keys !== null && keys.length === 0 && (
          <p className="url">
            Key management is locked — configure [auth].keys in orouta.toml first. A proxy without keys is open and can't be administered remotely.
          </p>
        )}
        {keys !== null && keys.length > 0 && (
          <form className="row" onSubmit={create}>
            <label>
              label
              <input type="text" value={label} onChange={(e) => setLabel(e.target.value)} />
            </label>
            <button className="btn" type="submit">
              Create key
            </button>
          </form>
        )}
      </div>
    </div>
  )
}
