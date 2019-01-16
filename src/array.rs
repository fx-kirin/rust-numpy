//! Untyped safe interface for NumPy ndarray

use npyffi;
use pyffi;
use cpython::*;
use ndarray::*;

use std::os::raw::c_void;
use std::ptr::null_mut;

use super::*;
use super::error::ArrayCastError;

/// Untyped safe interface for NumPy ndarray.
pub struct PyArray(PyObject);

impl PyArray {
    pub fn as_ptr(&self) -> *mut npyffi::PyArrayObject {
        self.0.as_ptr() as *mut npyffi::PyArrayObject
    }

    pub fn steal_ptr(self) -> *mut npyffi::PyArrayObject {
        self.0.steal_ptr() as *mut npyffi::PyArrayObject
    }

    pub unsafe fn from_owned_ptr(py: Python, ptr: *mut pyffi::PyObject) -> Self {
        let obj = PyObject::from_owned_ptr(py, ptr);
        PyArray(obj)
    }

    pub unsafe fn from_borrowed_ptr(py: Python, ptr: *mut pyffi::PyObject) -> Self {
        let obj = PyObject::from_borrowed_ptr(py, ptr);
        PyArray(obj)
    }

    /// The number of dimensions in the array
    ///
    /// https://docs.scipy.org/doc/numpy/reference/c-api.array.html#c.PyArray_NDIM
    pub fn ndim(&self) -> usize {
        let ptr = self.as_ptr();
        unsafe { (*ptr).nd as usize }
    }

    /// dimensions of the array
    ///
    /// https://docs.scipy.org/doc/numpy/reference/c-api.array.html#c.PyArray_DIMS
    pub fn dims(&self) -> Vec<usize> {
        let n = self.ndim();
        let ptr = self.as_ptr();
        let dims = unsafe {
            let p = (*ptr).dimensions;
            ::std::slice::from_raw_parts(p, n)
        };
        dims.into_iter().map(|d| *d as usize).collect()
    }

    pub fn len(&self) -> usize {
        self.dims().iter().fold(1, |a, b| a * b)
    }

    /// A synonym for PyArray_DIMS, named to be consistent with the ‘shape’ usage within Python.
    pub fn shape(&self) -> Vec<usize> {
        self.dims()
    }

    /// The number of elements matches the number of dimensions of the array
    ///
    /// - https://docs.scipy.org/doc/numpy/reference/c-api.array.html#c.PyArray_STRIDES
    /// - https://docs.scipy.org/doc/numpy/reference/generated/numpy.ndarray.strides.html#numpy.ndarray.strides
    pub fn strides(&self) -> Vec<isize> {
        let n = self.ndim();
        let ptr = self.as_ptr();
        let dims = unsafe {
            let p = (*ptr).strides;
            ::std::slice::from_raw_parts(p, n)
        };
        dims.into_iter().map(|d| *d as isize).collect()
    }

    unsafe fn data<T>(&self) -> *mut T {
        let ptr = self.as_ptr();
        (*ptr).data as *mut T
    }

    fn ndarray_shape<A>(&self) -> StrideShape<IxDyn> {
        // FIXME may be done more simply
        let shape: Shape<_> = Dim(self.shape()).into();
        let st: Vec<usize> =
            self.strides().iter().map(|&x| x as usize / ::std::mem::size_of::<A>()).collect();
        shape.strides(Dim(st))
    }

    pub fn typenum(&self) -> i32 {
        unsafe {
            let descr = (*self.as_ptr()).descr;
            (*descr).type_num
        }
    }

    fn type_check<A: types::TypeNum>(&self) -> Result<(), ArrayCastError> {
        let test = A::typenum();
        let truth = self.typenum();
        if A::typenum() == self.typenum() {
            Ok(())
        } else {
            Err(ArrayCastError::new(test, truth))
        }
    }

    /// Get data as a ndarray::ArrayView
    pub fn as_array<A: types::TypeNum>(&self) -> Result<ArrayViewD<A>, ArrayCastError> {
        self.type_check::<A>()?;
        unsafe { Ok(ArrayView::from_shape_ptr(self.ndarray_shape::<A>(), self.data())) }
    }

    /// Get data as a ndarray::ArrayViewMut
    pub fn as_array_mut<A: types::TypeNum>(&self) -> Result<ArrayViewMutD<A>, ArrayCastError> {
        self.type_check::<A>()?;
        unsafe { Ok(ArrayViewMut::from_shape_ptr(self.ndarray_shape::<A>(), self.data())) }
    }

    /// Get data as a Rust immutable slice
    pub fn as_slice<A: types::TypeNum>(&self) -> Result<&[A], ArrayCastError> {
        self.type_check::<A>()?;
        unsafe { Ok(::std::slice::from_raw_parts(self.data(), self.len())) }
    }

    /// Get data as a Rust mutable slice
    pub fn as_slice_mut<A: types::TypeNum>(&self) -> Result<&mut [A], ArrayCastError> {
        self.type_check::<A>()?;
        unsafe { Ok(::std::slice::from_raw_parts_mut(self.data(), self.len())) }
    }

    pub unsafe fn new_<T: TypeNum>(py: Python,
                                   np: &PyArrayModule,
                                   dims: &[usize],
                                   strides: *mut npy_intp,
                                   data: *mut c_void)
                                   -> Self {
        let dims: Vec<_> = dims.iter().map(|d| *d as npy_intp).collect();
        let ptr = np.PyArray_New(np.get_type_object(npyffi::ArrayType::PyArray_Type),
                                     dims.len() as i32,
                                     dims.as_ptr() as *mut npy_intp,
                                     T::typenum(),
                                     strides,
                                     data,
                                     0,                      // itemsize
                                     0,                      // flag
                                     ::std::ptr::null_mut()  //obj
                                     );
        Self::from_owned_ptr(py, ptr)
    }

    /// a wrapper of [PyArray_SimpleNew](https://docs.scipy.org/doc/numpy/reference/c-api.array.html#c.PyArray_SimpleNew)
    pub fn new<T: TypeNum>(py: Python, np: &PyArrayModule, dims: &[usize]) -> Self {
        unsafe { Self::new_::<T>(py, np, dims, null_mut(), null_mut()) }
    }

    /// a wrapper of [PyArray_ZEROS](https://docs.scipy.org/doc/numpy/reference/c-api.array.html#c.PyArray_ZEROS)
    pub fn zeros<T: TypeNum>(py: Python,
                             np: &PyArrayModule,
                             dims: &[usize],
                             order: NPY_ORDER)
                             -> Self {
        let dims: Vec<npy_intp> = dims.iter().map(|d| *d as npy_intp).collect();
        unsafe {
            let descr = np.PyArray_DescrFromType(T::typenum());
            let ptr = np.PyArray_Zeros(dims.len() as i32,
                                       dims.as_ptr() as *mut npy_intp,
                                       descr,
                                       order as i32);
            Self::from_owned_ptr(py, ptr)
        }
    }

    /// a wrapper of [PyArray_Arange](https://docs.scipy.org/doc/numpy/reference/c-api.array.html#c.PyArray_Arange)
    pub fn arange<T: TypeNum>(py: Python,
                              np: &PyArrayModule,
                              start: f64,
                              stop: f64,
                              step: f64)
                              -> Self {
        unsafe {
            let ptr = np.PyArray_Arange(start, stop, step, T::typenum());
            Self::from_owned_ptr(py, ptr)
        }
    }
}

impl<'source> FromPyObject<'source> for PyArray {
    fn extract(py: Python, obj: &'source PyObject) -> PyResult<Self> {
        Ok(obj.to_py_object(py).into_object().cast_into::<PyArray>(py)?)
    }
}

impl<'source> FromPyObject<'source> for &'source PyArray {
    fn extract(py: Python, obj: &'source PyObject) -> PyResult<Self> {
        Ok(obj.cast_as::<PyArray>(py)?)
    }
}

impl ToPyObject for PyArray {
    type ObjectType = Self;

    fn to_py_object(&self, py: Python) -> Self {
        PyClone::clone_ref(self, py)
    }
}

impl PythonObject for PyArray {
    #[inline]
    fn as_object(&self) -> &PyObject {
        &self.0
    }

    #[inline]
    fn into_object(self) -> PyObject {
        self.0
    }

    #[inline]
    unsafe fn unchecked_downcast_from(obj: PyObject) -> Self {
        PyArray(obj)
    }

    #[inline]
    unsafe fn unchecked_downcast_borrow_from<'a>(obj: &'a PyObject) -> &'a Self {
        ::std::mem::transmute(obj)
    }
}

impl PythonObjectWithCheckedDowncast for PyArray {
    fn downcast_from<'p>(py: Python<'p>,
                         obj: PyObject)
                         -> Result<PyArray, PythonObjectDowncastError<'p>> {
        //let np = PyArrayModule::import(py).unwrap();
        unsafe {
            //if npyffi::PyArray_Check(&np, obj.as_ptr()) != 0 {
                Ok(PyArray(obj))
            //} else {
                //Err(PythonObjectDowncastError(py))
            //}
        }
    }

    fn downcast_borrow_from<'a, 'p>(py: Python<'p>,
                                    obj: &'a PyObject)
                                    -> Result<&'a PyArray, PythonObjectDowncastError<'p>> {
        let np = PyArrayModule::import(py).unwrap();
        unsafe {
            if npyffi::PyArray_Check(&np, obj.as_ptr()) != 0 {
                Ok(::std::mem::transmute(obj))
            } else {
                Err(PythonObjectDowncastError(py))
            }
        }
    }
}
