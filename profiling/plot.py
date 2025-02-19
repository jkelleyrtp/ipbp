import matplotlib.pyplot as plt
import numpy as np
import pandas as pd
import json
import sys

fname = sys.argv[1]

items = []

# load file to json
data = json.loads(open(fname).read())

# get the total
total = data['total_time']['secs'] + data['total_time']['nanos'] / 1e9

for item in data['query_data']:
    items.append((item['label'], item['self_time']['secs'] + item['self_time']['nanos'] / 1e9))

df = pd.DataFrame(items)

# sort by time
df = df.sort_values(by=1, ascending=False)

df[2] = (df[1] * 1000).cumsum()
df[2] = df[2].max() - df[2]
df[2] = df[2] / df[2].max() * df[1].max() * 1000

df.index = df[0]


# rescale from seconds to milliseconds
df[1] = df[1] * 1000

df = df.head(50)
ax = df.plot.barh(figsize=(12, 8))

for idx, mls in enumerate(df[1]):
    pct = (mls / 1000) / total * 100
    ax.text(mls, idx, f'{pct:.2f}%', va='center', ha='left', color='black')



plt.ylabel('Item')
plt.xlabel('Milliseconds')
plt.title('Time spent in each item')


plt.tight_layout()


plt.show()
