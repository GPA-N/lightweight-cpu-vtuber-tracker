"""Cloud-only (Kaggle/Colab): MediaPipe Tasks reference weights for the spike test photos.

Usage in a notebook:
  !pip install -q mediapipe
  !wget -q https://storage.googleapis.com/mediapipe-models/face_landmarker/face_landmarker/float16/1/face_landmarker.task
  Upload the photos, then: !python mediapipe_reference.py test4.png test6.png ...
Prints the 52 blendshape scores per photo in MediaPipe order, for comparison with
docs/notes/accuracy-baseline.md.
"""
import sys

import mediapipe as mp
from mediapipe.tasks import python as mpt
from mediapipe.tasks.python import vision

opts = vision.FaceLandmarkerOptions(
    base_options=mpt.BaseOptions(model_asset_path="face_landmarker.task"),
    output_face_blendshapes=True,
    output_facial_transformation_matrixes=True,
    num_faces=1,
)
with vision.FaceLandmarker.create_from_options(opts) as lm:
    for path in sys.argv[1:]:
        res = lm.detect(mp.Image.create_from_file(path))
        print("==", path)
        if not res.face_blendshapes:
            print("NO FACE")
            continue
        for c in res.face_blendshapes[0]:
            print(f"{c.category_name} {c.score:.4f}")
        print("matrix", res.facial_transformation_matrixes[0].tolist())
