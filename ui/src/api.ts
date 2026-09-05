export class ApiError extends Error {
  status: number
  body: string

  constructor(status: number, body: string) {
    super(`api ${status}`)
    this.status = status
    this.body = body
  }
}

async function request(method: string, url: string, body?: unknown): Promise<Response> {
  const init: RequestInit = { method }
  if (body !== undefined) {
    init.headers = { "content-type": "application/json" }
    init.body = JSON.stringify(body)
  }
  const res = await fetch(url, init)
  if (!res.ok) {
    throw new ApiError(res.status, await res.text())
  }
  return res
}

export async function getJson<T>(url: string): Promise<T> {
  return (await request("GET", url)).json() as Promise<T>
}

export async function postJson<T>(url: string, body?: unknown): Promise<T> {
  return (await request("POST", url, body ?? {})).json() as Promise<T>
}

export async function del<T>(url: string): Promise<T> {
  return (await request("DELETE", url)).json() as Promise<T>
}
