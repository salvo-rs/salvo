let stopRecording = null
let upload = null
const recordBtn = document.querySelector('#record-btn')
const alertBox = document.querySelector('#support-alert')
const progressBox = document.querySelector('#progress-note')
const uploadList = document.querySelector('#upload-list')
const chunkInput = document.querySelector('#chunksize')
const endpointInput = document.querySelector('#endpoint')

if (!tus.isSupported) {
  alertBox.classList.remove('hidden')
}

if (!recordBtn) {
  throw new Error('Record button not found on this page. Aborting upload-demo.')
}

recordBtn.addEventListener('click', (e) => {
  e.preventDefault()
  if (stopRecording) {
    recordBtn.textContent = 'Start Recording'
    stopRecording()
  } else {
    recordBtn.textContent = 'Stop Recording'
    startStreamUpload()
  }
})

function startUpload(file) {
  const endpoint = endpointInput.value
  let chunkSize = Number.parseInt(chunkInput.value, 10)
  if (Number.isNaN(chunkSize)) {
    chunkSize = Number.POSITIVE_INFINITY
  }

  const options = {
    endpoint,
    chunkSize,
    retryDelays: [0, 1000, 3000, 5000],
    uploadLengthDeferred: true,
    metadata: {
      filename: 'webcam.webm',
      filetype: 'video/webm',
    },
    onError(error) {
      if (error.originalRequest) {
        if (window.confirm(`Failed because: ${error}\nDo you want to retry?`)) {
          upload.start()
          return
        }
      } else {
        window.alert(`Failed because: ${error}`)
      }

      reset()
    },
    onProgress(bytesUploaded) {
      progressBox.textContent = `Uploaded ${bytesUploaded} bytes so far.`
    },
    onSuccess() {
      const listItem = document.createElement('li')

      const video = document.createElement('video')
      video.controls = true
      video.src = upload.url
      listItem.appendChild(video)

      const lineBreak = document.createElement('br')
      listItem.appendChild(lineBreak)

      const anchor = document.createElement('a')
      anchor.textContent = `Download ${options.metadata.filename}`
      anchor.href = upload.url
      anchor.className = 'btn btn-success'
      listItem.appendChild(anchor)

      uploadList.appendChild(listItem)
      reset()
    },
  }

  upload = new tus.Upload(file, options)
  upload.start()
}

function startStreamUpload() {
  navigator.mediaDevices
    .getUserMedia({ video: true })
    .then((mediaStream) => {
      const mr = new MediaRecorder(mediaStream)

      stopRecording = () => {
        for (const t of mediaStream.getTracks()) {
          t.stop()
        }
        mr.stop()
        stopRecording = null
      }

      const readableStream = new ReadableStream({
        start(controller) {
          mr.ondataavailable = (event) => {
            event.data.bytes().then((buffer) => {
              controller.enqueue(buffer)
            })
          }

          mr.onstop = () => {
            controller.close()
          }

          mr.onerror = (event) => {
            controller.error(event.error)
          }

          mr.start(1000)
        },
        pull(_controller) {
          // Nothing to do here. MediaRecorder let's us know when there is new data available.
        },
        cancel() {
          mr.stop()
        },
      })

      startUpload(readableStream)
    })
    .catch(onError)
}

function reset() {
  upload = null
}

function onError(error) {
  console.log(error)
  alert(`An error occurred: ${error}`)

  upload = null
  stopRecording = null
  recordBtn.textContent = 'Start Recording'
}
