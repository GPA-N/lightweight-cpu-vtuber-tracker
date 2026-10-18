"""Side-by-side composite per photo and model: source photo with landmarks | avatar render.
usage: python -I scripts/photo_to_avatar_compose.py [OUT_DIR]
Reads OUT_DIR/{model}_{photo}.png, {model}_{photo}_lm.png and {model}_{photo}.txt, writes
OUT_DIR/composite_{model}_{photo}.png. The photo is mirrored like the camera preview, so the
avatar's screen-right arm matches the photo's screen-right arm.
"""
import os
import sys

import fitz

out_dir = sys.argv[1] if len(sys.argv) > 1 else "fixtures/private/out/photo_to_avatar"
H = 540
CAP = 70

for model in ("seed", "twist"):
    for photo in ("hands_15", "hands_16", "hands_17", "hands_19"):
        base = f"{out_dir}/{model}_{photo}"
        if not (os.path.exists(base + ".png") and os.path.exists(base + "_lm.png")):
            continue
        lm = fitz.Pixmap(base + "_lm.png")
        av = fitz.Pixmap(base + ".png")
        lw = round(lm.width * H / lm.height)
        aw = round(av.width * H / av.height)
        # Avatar render is 1280x720 with the model in the middle; keep the central 60 percent.
        doc = fitz.open()
        page = doc.new_page(width=lw + aw * 0.6, height=H + CAP)
        # Avatar first, shifted left under the photo; the page edge clips the right side.
        page.insert_image(fitz.Rect(lw - aw * 0.2, CAP, lw + aw * 0.8, CAP + H), filename=base + ".png")
        page.insert_image(fitz.Rect(0, CAP, lw, CAP + H), filename=base + "_lm.png")
        lines = [f"{model}  {photo}  (photo mirrored like the webcam preview; cyan = pose, green = hand detected; hand model PROVISIONAL, 2023 port)"]
        if os.path.exists(base + ".txt"):
            lines += [l.strip() for l in open(base + ".txt", encoding="utf-8", errors="replace")
                      if l.startswith(("LIMBS", "RENDERED", "  hand"))]
        y = 11
        for line in lines[:6]:
            page.insert_text((4, y), line[:230], fontsize=7)
            y += 10.5
        pix = page.get_pixmap(dpi=144)
        pix.save(f"{out_dir}/composite_{model}_{photo}.png")
        print("wrote", f"{out_dir}/composite_{model}_{photo}.png")

