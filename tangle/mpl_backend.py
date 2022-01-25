from matplotlib.backend_bases import FigureManagerBase
from matplotlib.backend_bases import Gcf
from matplotlib.backends.backend_agg import FigureCanvasAgg
import numpy as np
from math import floor
FigureCanvas = FigureCanvasAgg
from matplotlib import pyplot as plt
CURR_PLOTS = []

def show():
    global CURR_PLOTS
    """Show all figures as RGBA payloads sent to the egui::Images    """
    try:
        for figure_manager in Gcf.get_all_fig_managers():
            # if figure_manager.canvas.figure.get_axes():
            #     continue
            figure_manager.canvas.draw()
            buf = figure_manager.canvas.buffer_rgba()
            (width, height) = figure_manager.canvas.figure.bbox.size
            arr = np.frombuffer(buf, dtype=np.uint8).reshape((int(floor(height)), int(floor(width)), 4))
            CURR_PLOTS.append(arr)

            # display(
            #     figure_manager.canvas.figure,
            #     metadata=_fetch_figure_metadata(figure_manager.canvas.figure)
            # )
    finally:
        CURR_PLOTS = []
        # only call close('all') if any to close
        # close triggers gc.collect, which can be slow
        if Gcf.get_all_fig_managers():
            plt.close('all')

class FigureManager(FigureManagerBase):
    def show(self):
        raise NotImplemented()
