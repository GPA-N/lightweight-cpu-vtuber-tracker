"""Pixel error of our pose/hand landmarks against the official MediaPipe reference.
usage: python -I scripts/photo_to_avatar_compare.py PHOTO [OUT_DIR]   (PHOTO like hands_19)
Reads OUT_DIR/seed_PHOTO.json (our dump) and fixtures/private/out/mp_hands_ref/PHOTO.json.
"""
import json
import math
import sys

photo = sys.argv[1]
out_dir = sys.argv[2] if len(sys.argv) > 2 else "fixtures/private/out/photo_to_avatar"
ours = json.load(open(f"{out_dir}/seed_{photo}.json"))
ref = json.load(open(f"fixtures/private/out/mp_hands_ref/{photo}.json"))

names = {11: "L shoulder", 12: "R shoulder", 13: "L elbow", 14: "R elbow",
         15: "L wrist", 16: "R wrist", 19: "L index", 20: "R index"}
ref_pose = ref["pose"]["full"]["image"]
scale = math.hypot(ref["width"], ref["height"])
print(f"{photo} ({ref['width']}x{ref['height']}): pose px error vs official (pose_full)")
errs = []
for i, n in names.items():
    ox, oy, _, ov = ours["pose"][i]
    rx, ry, rv = ref_pose[i]["px"], ref_pose[i]["py"], ref_pose[i]["visibility"]
    e = math.hypot(ox - rx, oy - ry)
    errs.append(e)
    print(f"  {n:<11} ours ({ox:6.0f},{oy:6.0f}) vis {ov:.2f} | official ({rx:6.0f},{ry:6.0f}) vis {rv:.2f} | err {e:5.1f} px")
print(f"  mean limb error {sum(errs)/len(errs):.1f} px ({100*sum(errs)/len(errs)/scale:.1f}% of image diagonal)")
for i in (23, 24):
    print(f"  hip {i} vis ours {ours['pose'][i][3]:.2f} official {ref_pose[i]['visibility']:.2f} (never used)")

ref_hands = ref["hands"]["0.5"]["hands"]
print(f"hands: ours {len(ours['hands'])} (score>=0.5: {sum(h['score']>=0.5 for h in ours['hands'])}), official {len(ref_hands)}")
for h in ours["hands"]:
    if not ref_hands:
        break
    wx, wy = h["points"][0][0], h["points"][0][1]
    best = min(ref_hands, key=lambda r: math.hypot(r["image"][0]["px"] - wx, r["image"][0]["py"] - wy))
    per = [math.hypot(p[0] - q["px"], p[1] - q["py"]) for p, q in zip(h["points"], best["image"])]
    print(f"  ours {h['side']} score {h['score']:.2f} handedness-raw {h['handedness']:.2f} -> official {best['handedness']} {best['score']:.2f}: "
          f"wrist err {per[0]:.1f} px, index tip err {per[8]:.1f} px, mean of 21 {sum(per)/21:.1f} px")
