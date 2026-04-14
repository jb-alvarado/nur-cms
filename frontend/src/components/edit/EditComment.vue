<script setup lang="ts">
import { ref, computed } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { cloneDeep } from 'es-toolkit/object'
import { isEqual } from 'es-toolkit/predicate'
import { useI18n } from 'vue-i18n'
import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'
import { closeDropdown } from '@/utils/helper'
import { errMsg } from '@/utils/error'

import GenericModal from '@/components/generic/GenericModal.vue'

const { t } = useI18n()
const auth = useAuth()
const store = useIndex()
const route = useRoute()
const router = useRouter()

const deleteModal = ref()
const commentId = Number(route.params.id ?? 0)
const comment = ref({
    id: 0,
    entry_id: 0,
    parent_id: 0,
    user_id: 0,
    author_name: undefined,
    author_email: undefined,
    text: '',
    status: 'pending',
    entry: null,
} as CommentExt)
const commentOriginal = ref(cloneDeep(comment))
const needsSave = computed(() => !isEqual(comment.value, commentOriginal.value))
const status = ['pending', 'approved', 'rejected']

if (commentId > 0) {
    getComment()
}

const openDeleteModal = () => {
    deleteModal.value.showModal()
}

async function getComment() {
    await fetch(
        `/api/comments?id=${commentId}&fields=id,entry_id,parent_id,user_id,author_name,author_email,text,status,entry`,
        {
            headers: auth.authHeader,
        },
    )
        .then(async (resp) => {
            if (resp.status >= 400) {
                const msg = await errMsg(resp)
                throw new Error(msg)
            }

            return resp.json()
        })
        .then((response: RespondObj) => {
            comment.value = response.results[0]
            commentOriginal.value = cloneDeep(comment.value)
        })
        .catch((e) => {
            store.msgAlert('error', e)
        })
}

function commentDelete() {
    if (commentId > 0) {
        fetch(`/api/comments/${commentId}`, {
            method: 'DELETE',
            headers: auth.authHeader,
        })
            .then(async (resp) => {
                if (resp.status >= 400) {
                    const msg = await errMsg(resp)
                    throw new Error(msg)
                } else {
                    store.msgAlert('success', `Deleted: ${comment.value.author_name ?? comment.value.id}`)

                    router.push(`/comment`)
                }
            })
            .catch((e) => {
                store.msgAlert('error', e)
            })
    }
}

async function save() {
    const payload = Object.fromEntries(
        Object.entries(comment.value).filter(([key, value]) => {
            return !isEqual(value, commentOriginal.value[key as keyof CommentExt])
        }),
    )

    if (Object.keys(payload).length === 0) {
        store.msgAlert('warning', t('common.noChanges'))
        return
    }

    fetch(`/api/comments${commentId > 0 ? `/${commentId}` : ''}`, {
        method: commentId > 0 ? 'PUT' : 'POST',
        headers: {
            ...auth.authHeader,
            'Content-Type': 'application/json',
        },
        body: JSON.stringify(payload),
    })
        .then(async (resp) => {
            if (resp.status >= 400) {
                const msg = await errMsg(resp)
                throw new Error(msg)
            }
            store.msgAlert('success', t('common.saveSuccess'))

            if (commentId === 0) {
                await store.selectAuthors()
                router.push(`/comment/${await resp.text()}`)
            }
        })
        .catch((e) => {
            store.msgAlert('error', e)
        })
}
</script>

<template>
    <div class="flex flex-col pb-6">
        <div class="flex">
            <h1 class="grow text-2xl h-8">{{ comment?.author_name ?? '' }}</h1>
            <button class="btn btn-sm text-base" @click="router.back()">
                <i class="bi bi-chevron-left" />
            </button>
        </div>

        <!-- Form + Editor Container -->
        <div v-if="comment" class="flex flex-col flex-1 max-w-5xl bg-base-300 p-4 pt-1 mt-4 rounded">
            <div class="mt-2 py-1 px-2 rounded bg-base-200">
                {{ $t('comment.sourceArticle') }}:
                <RouterLink
                    :to="`/content/${comment.entry?.type}/${comment.entry?.id}`"
                    class="link hover:text-base-content/80"
                    >{{ comment.entry?.title ?? '' }}</RouterLink
                >
            </div>
            <!-- Form inputs -->
            <div class="flex items-center flex-wrap gap-2 flex-none">
                <div class="grow flex flex-col md:flex-row gap-2">
                    <fieldset class="fieldset max-w-80 md:max-w-56">
                        <legend class="fieldset-legend">{{ $t('comment.authorName') }}</legend>
                        <input
                            v-model="comment.author_name"
                            type="text"
                            class="input"
                            :placeholder="$t('comment.authorName')"
                        />
                    </fieldset>

                    <fieldset class="fieldset max-w-80 md:max-w-56">
                        <legend class="fieldset-legend">{{ $t('comment.authorEmail') }}</legend>
                        <input
                            v-model="comment.author_email"
                            type="email"
                            class="input"
                            :placeholder="$t('comment.authorEmail')"
                        />
                    </fieldset>
                </div>

                <div class="join md:mt-7">
                    <details class="dropdown">
                        <summary
                            class="btn join-item"
                            :class="{
                                'text-success': comment.status === 'approved',
                                'text-base-content/50': comment.status === 'pending',
                                'text-error': comment.status === 'rejected',
                            }"
                            @blur="closeDropdown"
                        >
                            {{ comment.status }}
                        </summary>
                        <ul class="menu dropdown-content bg-base-100 rounded-box z-1 w-24 p-2 shadow-sm">
                            <li
                                v-for="s in status"
                                :key="s"
                                :class="{
                                    'text-base-content/50': comment.status !== s,
                                }"
                            >
                                <a @click="comment.status = s">{{ s }}</a>
                            </li>
                        </ul>
                    </details>
                    <button class="btn text-warning join-item" @click="openDeleteModal()">
                        {{ $t('common.delete') }}
                    </button>
                    <button class="btn join-item" :class="{ 'btn-primary': needsSave }" @click="save()">
                        {{ $t('user.save') }}
                    </button>
                </div>
            </div>

            <div class="w-full">
                <fieldset class="fieldset">
                    <legend class="fieldset-legend">{{ $t('comment.text') }}</legend>
                    <textarea
                        v-model="comment.text"
                        class="textarea h-64 w-full"
                        :placeholder="$t('comment.text')"
                    ></textarea>
                </fieldset>
            </div>
        </div>

        <GenericModal ref="deleteModal" :title="$t('dialog.deleteTitle')" :ok-action="commentDelete">
            <p>{{ $t('comment.deleteConfirm') }}</p>
        </GenericModal>
    </div>
</template>
