//! ARP 收包路径与应答逻辑。

use log::{debug, info, warn};

use crate::driver::network::e1000::agreenment::{
    DstMac, EthHead, EtherType, Ipv4Addr, Net16, SourceMac,
};
use crate::driver::network::e1000::netbuffer::NetBuffer;
use crate::driver::network::e1000::packet::arp::cache::ARP_TABLE;
use crate::driver::network::e1000::packet::arp::packet::{
    ArpHeader, ARP_OPCODE_REPLY, ARP_OPCODE_REQUEST,
};
use crate::driver::network::e1000::tx_ringbuffer::{e1000_transmit, E1000TxRing};
use crate::driver::network::e1000::E1000_DEV;

/// 处理一个 ARP payload。
pub fn arp_receive(payload: NetBuffer, eth_hdr: EthHead) {
    let mut netb = payload;
    if netb.data_len() < core::mem::size_of::<ArpHeader>() {
        warn!("Bad ARP Packet, so short packet len,drop");
        return;
    }

    let arp_hdr = match netb.as_arp_header() {
        Some(hdr) => hdr,
        None => {
            warn!("ARP 头解析失败，直接丢弃");
            return;
        }
    };
    let arp_b = arp_hdr as *const ArpHeader as *const u8;

    let hw_type = arp_hdr.hw_type.host();
    let proto_type = arp_hdr.proto_type.host();
    let opcode = arp_hdr.opcode.host();
    // SAFETY: ArpHeader 固定 28 字节，以下偏移均对应协议定义字段。
    let sender_mac = unsafe { (arp_b.add(8) as *const [u8; 6]).read_unaligned() };
    let sender_ip = unsafe { (arp_b.add(14) as *const Ipv4Addr).read_unaligned() };
    let target_ip = unsafe { (arp_b.add(24) as *const Ipv4Addr).read_unaligned() };

    info!(
        "ARP packet: hw_type={} proto={:#06x} hw_len={} proto_len={} opcode={} ({}) payload={} bytes",
        hw_type,
        proto_type,
        arp_hdr.hw_len.host(),
        arp_hdr.proto_len.host(),
        opcode,
        if opcode == ARP_OPCODE_REQUEST {
            "Request"
        } else if opcode == ARP_OPCODE_REPLY {
            "Reply"
        } else {
            "Unknown"
        },
        netb.data_len(),
    );

    if hw_type != 1
        || proto_type != 0x0800
        || arp_hdr.hw_len.host() != 6
        || arp_hdr.proto_len.host() != 4
    {
        warn!("不支持的 ARP 类型组合，丢弃");
        return;
    }

    match opcode {
        ARP_OPCODE_REQUEST => {
            debug!(
                "Recive ARP Request: who is {:?} ? (from {:?} / {:02x?})",
                target_ip, sender_ip, sender_mac
            );

            ARP_TABLE.lock().insert(sender_ip, DstMac(sender_mac));

            if target_ip == Ipv4Addr::new([10, 0, 0, 2]) {
                info!("Arp target is me! prepare to ARP Reply...");
                let mut rep_arp = *arp_hdr;
                rep_arp.opcode = Net16::new(ARP_OPCODE_REPLY);
                let mut dev_lock = E1000_DEV.lock();
                rep_arp.target_mac = rep_arp.sender_mac;
                rep_arp.sender_mac = (*dev_lock).as_ref().expect("no dev").mac;
                let reply_sender_ip = rep_arp.sender_ip;
                rep_arp.sender_ip = rep_arp.target_ip;
                rep_arp.target_ip = reply_sender_ip;

                let mut new_buffer = NetBuffer::new();
                new_buffer.new_data(rep_arp.to_be_bytes());

                let rep_ethdr = EthHead::new(
                    DstMac(rep_arp.target_mac),
                    SourceMac(rep_arp.sender_mac),
                    EtherType::ARP,
                );

                info!("ARP header:{:?}", rep_arp);
                new_buffer.new_agreement_head(&rep_ethdr);

                let tx_ring = unsafe {
                    &mut *((&mut (*dev_lock).as_mut().expect("nodev").tx_ring) as *mut E1000TxRing)
                };
                e1000_transmit(
                    &(*dev_lock).as_ref().expect("nodev").bar,
                    tx_ring,
                    new_buffer,
                );
            } else {
                debug!("Arp target not is me,ignore it。");
            }
        }
        ARP_OPCODE_REPLY => {
            info!(
                "Recive ARP replay from : {:?}  MAC is {:02x?}",
                sender_ip, sender_mac
            );
            ARP_TABLE.lock().insert(sender_ip, DstMac(sender_mac));
        }
        _ => warn!("未知的 ARP 操作码: {}", opcode),
    }
}

/// 发送 ARP 请求。
pub fn send_arp_packet(target_ip: Ipv4Addr) {
    // 获取本机 MAC
    let local_mac = {
        let guard = E1000_DEV.lock();
        guard.as_ref().expect("no e1000 device").mac
    };

    // 假设 MY_IPV4 是你全局定义的本机 IP
    let local_ip = Ipv4Addr::new([10, 0, 0, 2]);

    // 1. 创建一个新的 ARP Request 包
    let arp_req = ArpHeader::new(
        1,
        0x0800,
        6,
        4,
        ARP_OPCODE_REQUEST,
        local_mac,
        local_ip,
        [0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        target_ip,
    );

    // 2. 创建新的 eth 广播头
    let eth_hdr = EthHead::new(
        DstMac([0xff, 0xff, 0xff, 0xff, 0xff, 0xff]),
        SourceMac(local_mac),
        EtherType::ARP,
    );

    // 3. 创建新的 netbuffer, 然后缝合协议
    let mut buffer = NetBuffer::new();

    // 把 ARP 结构体当作 Payload 塞进去
    // （假设你之前是通过 unsafe 转 slice，或者你的 NetBuffer 有直接接受结构体引用的宏/方法）
    let arp_slice = unsafe {
        core::slice::from_raw_parts(
            &arp_req as *const _ as *const u8,
            core::mem::size_of::<ArpHeader>(),
        )
    };
    buffer.new_data(arp_slice);

    // 像你之前一样，压入以太网头部
    buffer.new_agreement_head(&eth_hdr);

    // 4. 发送包
    let mut guard = E1000_DEV.lock();
    let dev = (*guard).as_mut().expect("no e1000 device");

    // 强制转换以绕过借用检查器（和你在 ipv4 里写的一样）
    let tx_ring = unsafe { &mut *((&mut dev.tx_ring) as *mut _ as usize as *mut E1000TxRing) };

    e1000_transmit(&dev.bar, tx_ring, buffer);

    info!(
        "发出了 ARP Request: Who is {:?} ? Tell {:?}",
        target_ip, local_ip
    );
}
