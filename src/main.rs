#![no_std]
#![no_main]

extern crate panic_halt;

use cortex_m_rt::{entry, exception};
use stm32h7::stm32h747cm7;
use cortex_m::peripheral::syst::SystClkSource;
use cortex_m::peripheral::SYST;

const CORE_CLOCK: u32 = 480_000_000;
const TICK_FREQ: u32 = 1_000;

static mut TICK: u32 = 0;

#[entry]
fn main() -> ! {
    let core_periph = cortex_m::Peripherals::take().unwrap();
    let peripherals = stm32h747cm7::Peripherals::take().unwrap();

    // Use SMPS direct
    let pwr = &peripherals.PWR;
    pwr.cr3.modify(|r, w| unsafe { w.bits(r.bits() & 0xffff_ffc4) });

    // Set voltage scale to VOS1 (required before going to overdrive (VOS0))
    pwr.d3cr.modify(|_, w| unsafe { w.vos().bits(0x03) });
    while !pwr.d3cr.read().vosrdy().bit_is_set() {}

    // Enable overdrive mode (VS0)
    let rcc = &peripherals.RCC;
    rcc.apb4enr.modify(|_, w| w.syscfgen().set_bit());
    let syscfg = &peripherals.SYSCFG;
    syscfg.pwrcr.modify(|_, w| w.oden().enabled());
    while !pwr.d3cr.read().vosrdy().bit_is_set() {}

    // Set PLL clock source to HSI and set prescaler to divide by 4
    // HSI = 64MHz & max allowed PLL src clock = 16MHz, so 64MHz / 4 = 16MHz
    rcc.pllckselr.modify(|_, w| {
        w.pllsrc().hsi()
            .divm1().bits(0x4)
    });

    // Enable PLL1-P clock
    // Disable PLL1-Q & PLL1-R clocks (not used)
    // Disable PLL-1 Fraction (i.e. integer scaling)
    // Set VCOSEL based on ref_clk (16MHz)
    // Set PLL1 range based on ref_clk (16MHz)
    rcc.pllcfgr.modify(|_, w| {
        w.divp1en().enabled()
            .divq1en().disabled()
            .divr1en().disabled()
            .pll1fracen().clear_bit()
            .pll1vcosel().wide_vco()
            .pll1rge().range8()
    });

    // Set PLL1-P division = 1 (pll1_p_ck = vco1_ck)
    // Set PLL-1 multiplication factor = 30
    // VCO = ref_clk x divn = 16MHz x 30 = 480MHz
    rcc.pll1divr.modify(|_, w| unsafe {
        w.divp1().div1()
            .divn1().bits(0x1e)
    });

    // Enable PLL-1 and wait for it to become ready
    rcc.cr.modify(|_, w| w.pll1on().on());
    while !rcc.cr.read().pll1rdy().bit_is_set() {}

    // Set sys_clk div1 (i.e. same as VCO - 480MHz)
    // Set D1 Prescaler to div 2 = 480MHz / 2 = 240MHz
    rcc.d1cfgr.modify(|_, w| {
        w.d1cpre().div1()
            .hpre().div2()
    });

    // Set sys_clk = PLL1
    rcc.cfgr.modify(|_, w| w.sw().pll1());

    // Check to ensure sys_clk is now PLL1
    while rcc.cfgr.read().sws().bits() != 0x3 {}

    // Set flash latency based on new AXI bus clock and voltage scale
    let flash = &peripherals.FLASH;
    flash.acr.modify(|_, w| unsafe {
        w.wrhighfreq().bits(0x02)
            .latency().bits(0x04)
    });

    // Check the latency has been accepted
    while flash.acr.read().wrhighfreq().bits() != 0x02 &&
          flash.acr.read().latency().bits() != 0x04 {}

    // Set SysTick every 1ms
    let mut syst = core_periph.SYST;
    configure_systick(CORE_CLOCK / TICK_FREQ, &mut syst);

    // LED2 connected to PE1
    let gpioe = &peripherals.GPIOE;
    rcc.ahb4enr.modify(|_, w|  w.gpioeen().set_bit());
    gpioe.moder.modify(|_, w| w.moder1().output());

    // Flash on for 1s, off for 1s
    loop {
        gpioe.odr.modify(|_, w| w.odr1().set_bit());
        delay(1000);

        gpioe.odr.modify(|_, w| w.odr1().clear_bit());
        delay(1000);
    }
}

#[inline(never)]
fn configure_systick(reload: u32, syst: &mut SYST) {
    syst.set_clock_source(SystClkSource::Core);
    syst.set_reload(reload-1);
    syst.clear_current();
    syst.enable_counter();
    syst.enable_interrupt();
}

#[inline(never)]
fn delay(wait: u32) {
    let start = get_tick();
     while (get_tick() - start) < wait {}
}

#[exception]
fn SysTick() {
    unsafe { TICK += 1 }
}

fn get_tick() -> u32 {
    unsafe { TICK }
}