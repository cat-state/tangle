import inspect
import dis
from typing import Any, Dict, List
import textwrap

import matplotlib
matplotlib.use("module://tangle.mpl_backend")
import matplotlib.pyplot as plt
from .mpl_backend import CURR_PLOTS

def get_closurevars(fn):
    return inspect.getclosurevars(fn)


from functools import wraps

GUI_REF = None
UI_REF = None

def set_gui_ref(gui_ref):
    global GUI_REF
    GUI_REF = gui_ref


def set_ui_ref(ui_ref):
    global UI_REF
    UI_REF = ui_ref

class UiWrapper():
    def __getattr__(self, name: str) -> Any:
        return getattr(UI_REF, name)


def eq(x, y):
    if x is y:
        return True
    if isinstance(x, (int, float, str)) and isinstance(y, (int, float, str)):
        return x == y
    else:
        return False

def memo(name):
    def do_memo(fn):
        prev_args = None
        def memoized(*args):
            nonlocal prev_args
            # it has to be any(not) becaue we want to find the first mismatching arg
            # not any() would find the first matching arg, but would ignore future
            # mismatching arguments
            if prev_args is None or any(not eq(arg, prev_arg) for arg, prev_arg in zip(args, prev_args)):
                prev_args = args
                #py
                memoized._changed = True
                memoized._prev_return = try_or_exn(fn, *args)
                return memoized._prev_return
            else:
                memoized._changed = False
                return memoized._prev_return

        memoized._prev_return = []
        memoized._changed = False
        curr_plots = []
        def wrapped(*args):
            def show(ui):
                nonlocal curr_plots
                global CURR_PLOTS

                set_ui_ref(ui)
                CURR_PLOTS.clear()
                plt.clf()
                ret = memoized(*args)
                if memoized._changed:
                    print(name, memoized._changed, CURR_PLOTS)
                    curr_plots = CURR_PLOTS[:]

                    
                if isinstance(ret, Exception):
                    ui.visualize_py(f"{name}_dbg_error", ret)
                    raise ret

                for i, img in enumerate(curr_plots):
                    print("imaging, i")
                    ui.image(f"{name}_{i}_plot", img)
                
                # i = 0
                # for output in ret:
                #     ui.visualize_py(f"{name}_dbg_{i}", output)
                #     i += 1
                return ret

            prev_return = memoized._prev_return
            ret = GUI_REF.tangle_node_output(name, show)

            if not (ret is prev_return):
                for i, (r, pr) in enumerate(zip(ret, prev_return)):
                    if not eq(r, pr):
                        print(name, i)
                        GUI_REF.output_changed(name, i)
            return ret

        
        return wrapped
    return do_memo

def cell_load_globals(code: str, exclude_globals: Dict[str, Any]):
    if code.rstrip() == "":
        return []
    indented = textwrap.indent(code, "    ")
    func = f"def cell():\r\n{indented}"
    code = compile(func, "<cell>", "exec")
    cell_code = next(dis.get_instructions(code)).argval
    gbs = set()
    for ins in dis.get_instructions(cell_code):
        
        if ins.opname == "LOAD_GLOBAL" and ins.argval not in exclude_globals:
            gbs.add(ins.argval)
    return list(gbs)

def compile_cell(name, code: str, provides):
    code = code.rstrip()
    lines = code.splitlines()

    if len(lines) == 0:
        lines = ["None"]
    if len(provides) == 0:
        provides = ["__retval"]
        lines[-1] = f"__retval = {lines[-1]}"
    
    fn_body = [f"    {line}" for line in lines]
    test_body = ["def __test_fn__():"] + fn_body
    scope = {}
    exec("\n".join(test_body), scope)
    unbound = get_closurevars(scope["__test_fn__"]).unbound
    fn = scope["__test_fn__"]
    gbs = set()
    for ins in dis.get_instructions(fn):
        if ins.opname == "LOAD_GLOBAL" and ins.argval in unbound:
            gbs.add(ins.argval)
    gbs = list(gbs)
    fn_body.append(f"    return ({','.join(provides)}{',' if len(provides) == 1 else ''})")
    cell_body = f"def {name}({','.join(gbs)}):" + "\n" + '\n'.join(fn_body)
    print(cell_body, gbs)
    scope = {}
    exec(cell_body, scope)

    return scope[name], gbs

def eval_graph(curr, graph, cache, inverted_provides, provides):
    if curr in cache:
        return cache[curr]
    else:
        try:
            node = inverted_provides[curr]
            fn_or_exn = graph[node]
        except KeyError:
            raise SyntaxError(f"No variable '{curr}'")
        if isinstance(fn_or_exn, Exception):
            raise fn_or_exn
        fn, args = fn_or_exn
        print('ip', inverted_provides, curr, graph, cache, args)
        kwargs = {k:eval_graph(k, graph, cache, inverted_provides, provides) for k in args}
        val = fn(**kwargs)  
        
        for p, v in zip(provides[node], val):
            cache[p] = v
        return val

def try_or_exn(fn, *args, **kwargs):
    try:
        return fn(*args, **kwargs)
    except Exception as e:
        return e

def compile_and_run(cells: Dict[str, str], cache, provides):
    compiled = {name:try_or_exn(compile_cell, name, cell, provides[name]) for name, cell in cells.items()}
    inverted_provides = {}
    for n, pv in provides.items():
        print(pv, provides)
        for k in pv:
            inverted_provides[k] = n
        inverted_provides[n] = n
    for name in compiled.keys():
        cache[name] = try_or_exn(eval_graph, name, compiled, cache, inverted_provides, provides)
        cells[name] = compiled[name]
    return cache