from matplotlib.backend_bases import FigureManagerBase
from matplotlib.backend_bases import Gcf
from matplotlib.backends.backend_agg import FigureCanvasAgg
import numpy as np
from math import floor
FigureCanvas = FigureCanvasAgg

CURR_PLOTS = []
def show(*args, **kwargs):
    global CURR_PLOTS

    for num, figmanager in enumerate(Gcf.get_all_fig_managers()):
        figmanager.canvas.draw()
        buf = figmanager.canvas.buffer_rgba()
        (width, height) = figmanager.canvas.figure.bbox.size
        arr = np.frombuffer(buf, dtype=np.uint8).reshape((int(floor(height)), int(floor(width)), 4))
        CURR_PLOTS.append(arr)
        print("hiolla", len(CURR_PLOTS))

        figmanager.canvas.figure.savefig(f"figure_{num}.png")

class FigureManager(FigureManagerBase):
    def show(self):
        print("oh")
        self.canvas.figure.savefig('foo.png')

