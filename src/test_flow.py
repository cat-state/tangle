def node_1():
    x = 1
    k = 1
    return [x, k]

def node_2(gui):
    y = gui.slider("Y")
    return y

def node_3(x, y):
    z = x / y
    return z

def node_4():
    import numpy as np
    return np

def node_5(np, z):
    wave = np.sin(np.linspace(-z, z))
    return wave

def node_6():
    import matplotlib.pyplot as plt
    return plt

def node_7(plt, wave):
    plots = plt.plot(wave)
    return plots

def dataflow(gui):
    [x, k] = node_1()
    y = node_2(gui)
    z = node_3(x, y)
    np = node_4()
    wave = node_5(np, z)
    plt = node_6()
    plots = node_7(plt, wave)

    return z

