import { useState } from "react"
import Keys from "./Keys"
import Login from "./Login"
import Status from "./Status"

export default function App() {
  const path = window.location.pathname
  const [needLogin, setNeedLogin] = useState(false)
  if (needLogin || path === "/login") return <Login />
  if (path === "/keys") return <Keys onUnauthorized={() => setNeedLogin(true)} />
  return <Status onUnauthorized={() => setNeedLogin(true)} />
}
