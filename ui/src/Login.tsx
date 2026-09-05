import { useEffect, useState, type FormEvent } from "react"
import { getJson, postJson } from "./api"
import Header from "./Header"

export default function Login() {
  const [key, setKey] = useState("")
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    getJson("/api/keys")
      .then(() => window.location.replace("/status"))
      .catch(() => {})
  }, [])

  async function submit(e: FormEvent) {
    e.preventDefault()
    try {
      await postJson("/api/login", { key: key.trim() })
      window.location.href = "/status"
    } catch {
      setError("invalid api key")
    }
  }

  return (
    <div className="wrap">
      <Header title="/ login" active="login" />
      <form className="add login" onSubmit={submit}>
        <h2>API key</h2>
        <div className="row">
          <label>
            key
            <input
              type="password"
              value={key}
              onChange={(e) => setKey(e.target.value)}
              autoFocus
            />
          </label>
          <button className="btn" type="submit">
            Sign in
          </button>
        </div>
        {error && <p className="url login-err">{error}</p>}
      </form>
    </div>
  )
}
