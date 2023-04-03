#!/usr/bin/env python
import json
import re

pre = json.load(open('src/pre.json'))
post = json.load(open('src/post.json'))

compnames = ['Position', 'Size', 'Rotation', 'Renderable', 'SpawnPoint', 'LevelData', 'RBPosition', 'RBSize', 'RBRotation']
compnames = set(compnames)

def filter_json(val, keep):
    if not isinstance(val, dict):
        return val
    ret = dict()
    for k in val.keys():
        if not (k in compnames and (k not in keep)):
            ret[k] = filter_json(val[k], keep)
    return ret

keep = set(['RBRotation'])
ret = filter_json(pre, keep)
with open('src/pre2.json', 'w') as f:
    json.dump(ret, f)

keep2 = set([])
ret = filter_json(post, keep)
with open('src/post2.json', 'w') as f:
    json.dump(ret, f)

