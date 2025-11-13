<script setup lang="ts">
import { type ModelRef, ref } from 'vue'
const path: ModelRef<string | undefined> = defineModel('path')
const imageFile: ModelRef<string | undefined> = defineModel('image')

const imagePlaceholder = ref(null as null | string)

defineProps({
    title: {
        type: String,
        default: 'Image',
    },
    placeholder: {
        type: String,
        default: 'Thumbnail',
    },
})

function removeImage() {
    path.value = undefined
    imageFile.value = undefined
    imagePlaceholder.value = null
}

function onFileChange(evt: any) {
    const files = evt.target.files || evt.dataTransfer.files

    if (files.length > 0) {
        path.value = undefined
        imageFile.value = files[0]
        imagePlaceholder.value = files[0].name
    }
}
</script>

<template>
     <fieldset class="fieldset w-full">
        <legend class="fieldset-legend">{{ title }}</legend>
        <div class="join">
            <input
                v-model="path"
                type="text"
                :placeholder="imagePlaceholder || placeholder"
                class="input input-bordered join-item w-full"
            />
            <label for="fileInput" class="btn px-2 join-item">
                <i class="bi bi-file-earmark-plus text-lg"></i>
            </label>
            <button class="btn px-2 join-item" @click.prevent="removeImage">
                <i class="bi bi-file-earmark-minus text-lg"></i>
            </button>
        </div>
        <input id="fileInput" type="file" class="hidden" accept=".jpg, .jpeg, .png" @change="onFileChange" />
    </fieldset>
</template>
