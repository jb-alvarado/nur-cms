class NurCmsEcho extends HTMLElement {
    connectedCallback() {
        this.render()
        this.querySelector('[data-action="request"]')?.addEventListener('click', () => this.request())
    }

    render() {
        this.replaceChildren()

        const card = document.createElement('section')
        card.className = 'nur-echo-card'
        const body = document.createElement('div')
        body.className = 'nur-echo-card-body'

        const title = document.createElement('h1')
        title.className = 'nur-echo-title'
        title.textContent = 'Echo plugin'

        const description = document.createElement('p')
        description.textContent = 'This admin web component uses the CMS-provided authenticated API client.'

        const button = document.createElement('button')
        button.type = 'button'
        button.className = 'nur-echo-button'
        button.dataset.action = 'request'
        button.textContent = 'Call protected plugin route'

        const result = document.createElement('pre')
        result.dataset.result = ''
        result.className = 'nur-echo-result'
        result.textContent = 'No request made yet.'

        body.append(title, description, button, result)
        card.append(body)
        this.append(card)
    }

    async request() {
        const result = this.querySelector('[data-result]')
        const button = this.querySelector('[data-action="request"]')
        if (!result) return
        if (!this.context?.request) {
            result.textContent = 'The CMS API context is unavailable.'
            return
        }

        result.textContent = 'Loading…'
        if (button instanceof HTMLButtonElement) button.disabled = true
        try {
            const response = await this.context.request('/api/plugins/echo/editor')
            const body = await response.text()
            if (!response.ok) {
                throw new Error(body || `Request failed with status ${response.status}.`)
            }
            result.textContent = body
        } catch (error) {
            result.textContent = error instanceof Error ? error.message : 'The request failed.'
        } finally {
            if (button instanceof HTMLButtonElement) button.disabled = false
        }
    }
}

customElements.define('nur-cms-echo', NurCmsEcho)
