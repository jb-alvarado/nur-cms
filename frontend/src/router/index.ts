import { createRouter, createWebHistory } from 'vue-router'
import HomeView from '../views/HomeView.vue'
import LoginView from '@/views/LoginView.vue'

import { useAuth } from '@/stores/auth'
import { useIndex } from './../stores/index'

const router = createRouter({
    history: createWebHistory(import.meta.env.BASE_URL),
    routes: [
        {
            path: '/login',
            name: 'login',
            component: LoginView,
            meta: { public: true, showMenu: false },
        },
        {
            path: '/',
            name: 'home',
            component: HomeView,
            meta: { showMenu: true },
        },
        {
            path: '/verification',
            name: 'verification',
            component: () => import('../views/VerificationView.vue'),
            meta: { public: true, showMenu: false },
        },
        {
            path: '/configuration',
            name: 'configuration',
            component: () => import('../views/ConfigurationView.vue'),
            meta: { showMenu: true },
        },
        {
            path: '/author',
            name: 'author',
            component: () => import('../views/author/IndexView.vue'),
            meta: { showMenu: true },
        },
        {
            path: '/author/:id',
            name: 'author edit',
            component: () => import('../views/author/EditView.vue'),
            meta: { showMenu: true },
        },
        {
            path: '/category',
            name: 'category',
            component: () => import('../views/category/IndexView.vue'),
            meta: { showMenu: true },
        },
        {
            path: '/category/:id',
            name: 'category edit',
            component: () => import('../views/category/EditView.vue'),
            meta: { showMenu: true },
        },
        {
            path: '/category/:id/:group_id',
            name: 'group category edit',
            component: () => import('../views/category/EditView.vue'),
            meta: { showMenu: true },
        },
        {
            path: '/content/event',
            name: 'content event',
            component: () => import('../views/event/IndexView.vue'),
            meta: { showMenu: true },
        },
        {
            path: '/content/event/:id',
            name: 'event edit',
            component: () => import('../views/event/EditView.vue'),
            meta: { showMenu: true },
        },
        {
            path: '/content/event/:id/:group_id',
            name: 'group event edit',
            component: () => import('../views/event/EditView.vue'),
            meta: { showMenu: true },
        },
        {
            path: '/content/:type',
            name: 'content type',
            component: () => import('../views/content/IndexView.vue'),
            meta: { showMenu: true },
        },
        {
            path: '/content/:type/:id',
            name: 'content edit',
            component: () => import('../views/content/EditView.vue'),
            meta: { showMenu: true },
        },
        {
            path: '/content/:type/:id/:group_id',
            name: 'group content edit',
            component: () => import('../views/content/EditView.vue'),
            meta: { showMenu: true },
        },
        {
            path: '/comment',
            name: 'comment',
            component: () => import('../views/comment/IndexView.vue'),
            meta: { showMenu: true },
        },
        {
            path: '/comment/:id',
            name: 'comment edit',
            component: () => import('../views/comment/EditView.vue'),
            meta: { showMenu: true },
        },
        {
            path: '/media',
            name: 'media',
            component: () => import('../views/MediaView.vue'),
            meta: { showMenu: true },
        },
        {
            path: '/user',
            name: 'user',
            component: () => import('../views/UserView.vue'),
            meta: { showMenu: true },
        },
        {
            path: '/:pathMatch(.*)*',
            name: '404',
            component: () => import('../views/404NotFount.vue'),
            alias: '/404',
            meta: { public: true, showMenu: false },
        },
    ],
})

router.beforeEach(async (to, from) => {
    const auth = useAuth()
    const store = useIndex()
    await auth.inspectToken()

    if (
        (to.path.startsWith('/author') && !from.path.startsWith('/author')) ||
        (to.path.startsWith('/category') && !from.path.startsWith('/category')) ||
        (to.path.startsWith('/content') && !from.path.startsWith('/content')) ||
        (to.path.startsWith('/comment') && !from.path.startsWith('/comment')) ||
        (to.path.startsWith('/media') && !from.path.startsWith('/media'))
    ) {
        store.search = ''
    }

    const isPublicRoute = to.meta.public === true

    if (!auth.isLogin && !isPublicRoute) {
        return { name: 'login' }
    }

    if (auth.isLogin && isPublicRoute && to.name !== '404') {
        return { name: 'home' }
    }

    return
})

export default router
