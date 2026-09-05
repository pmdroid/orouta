import type { ReactNode } from "react"
import type { Tailscale } from "./types"

type Props = {
  title: string
  active: "status" | "keys" | "login"
  sub?: ReactNode
}

export default function Header({ title, active, sub }: Props) {
  return (
    <>
      <header>
        <img className="logo" src="/logo.png" alt="orouta" />
        <h1>
          <span>{title}</span>
        </h1>
        {active !== "login" && (
          <nav>
            <a href="/status" className={active === "status" ? "active" : undefined}>
              hosts
            </a>
            {" · "}
            <a href="/keys" className={active === "keys" ? "active" : undefined}>
              api keys
            </a>
          </nav>
        )}
      </header>
      {sub && <p className="sub">{sub}</p>}
    </>
  )
}

export function TsChip({ ts }: { ts: Tailscale | null }) {
  if (!ts) return null
  if (ts.serving) {
    const url = ts.url ?? `https://${ts.self}`
    return (
      <span className="ts">
        <b>TAILSCALE</b> <a href={url}>{url}</a>
      </span>
    )
  }
  return <span className="ts dim">TAILSCALE · {ts.online ? "no serve" : "offline"}</span>
}
