import { defineCustomElement } from 'vue'

import AdminPlugin from './AdminPlugin.ce.vue'

const elementName = 'nur-cms-vue-admin'

if (!customElements.get(elementName)) {
    customElements.define(elementName, defineCustomElement(AdminPlugin))
}
