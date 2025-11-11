export async function errMsg(resp: Response): Promise<string> {
    const text = await resp.text()
    let msg: string

    try {
        const json = JSON.parse(text)
        msg = json?.error ?? (typeof json === 'string' ? json : JSON.stringify(json))
    } catch {
        msg = text
    }

    if (!msg) {
        msg = resp.statusText
    }

    return msg
}
