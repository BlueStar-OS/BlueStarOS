const  MSEC:usize=1000;
use riscv::register::time;
use crate::sbi::set_next_timetriger;
use crate::config::{CPU_CIRCLE, TIME_FREQUENT};
use log::debug;


#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct TimeVal{
    pub sec:usize,
    pub usec:usize,
}


///返回tick数

pub fn get_time_tick()->usize{
    time::read()
}


///返回毫秒数
pub fn get_time_ms()->usize{
    let current=(time::read()*MSEC)/CPU_CIRCLE;//先×再除防止精度丢失
    current
}


///设置下一次时钟中断(不带中断检查，太耗时间，所有耗时操作其实都不应该出现在这里)，mtimecmp使用原始tick计数
pub fn set_next_timeInterupt(){
    //需要考虑调用误差，即使错过也没事，只是提前触发中断(mtime < mtimecmp)
    let next_time=get_time_tick() + CPU_CIRCLE/TIME_FREQUENT;
    set_next_timetriger(next_time);
}

///内核sleep函数,传入毫秒数 阻塞式  目前不能使用，buged
pub fn kernel_sleep(time_ms:usize){
let target =time::read()+CPU_CIRCLE/MSEC*time_ms;
    while time::read()<= target {
      //  debug!("current :{} targer :{}",time::read(),target)
      core::hint::spin_loop();
    }

}
