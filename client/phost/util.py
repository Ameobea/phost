""" Various utility and helper functions """


from functools import reduce
import uuid


class Composition(object):
    def __init__(self, inner_function, wrappers):
        self.composed = reduce(
            lambda acc, wrapper: wrapper(acc), reversed(wrappers), inner_function
        )

        # Click holds metadata about what kinds of arguments, options, etc. have been added to
        # its commands in this attribute.  It is accumulated during the composition reduction.
        # By setting it here, `click` gets access to all of the metadata accumulated by the
        # wrapper functions via the instance of this class that is returned.
        self.__click_params__ = getattr(self.composed, "__click_params__", None)

    def __call__(self, *args, **kwargs):
        return self.composed(*args, **kwargs)


def compose(*wrappers):
    """ Composes all provided decorator functions from right to left (right-most is outermost) and
    returns a single function that applies them all as a decorator.

    Example:
        >>> def wrap(i: int):
        ...     def wrapper(func):
        ...         def inner(*args, **kwargs):
        ...             print('inner {}'.format(i))
        ...             return func(*args, **kwargs)
        ...
        ...         return inner
        ...
        ...     return wrapper
        ...
        >>> @compose(wrap(0), wrap(1), wrap(2))
        ... def foo():
        ...     print('foo')
        ...
        >>> foo()
        inner 0
        inner 1
        inner 2
        foo
        >>>
    """

    return lambda func: Composition(func, wrappers)


def slugify(s: str) -> str:
    return s.lower().replace(" ", "-").replace(".", "_")


def create_random_subdomain() -> str:
    return uuid.uuid4().hex[:16]


def test_compose():
    def wrap(i: int):
        def wrapper(func):
            def inner(*args, **kwargs):
                return ["inner {}".format(i), *func(*args, **kwargs)]

            return inner

        return wrapper

    @compose(wrap(0), wrap(1), wrap(2))
    def foo():
        return ["foo"]

    assert foo() == ["inner 0", "inner 1", "inner 2", "foo"]
